//! Origin authorization: the origin class of every navigation target and
//! the per-hop verdict, plus the per-session grants and the policy
//! snapshot the synchronous engine navigation hook decides from.
//!
//! Classes are computed on the engine-canonical host (the WHATWG URL
//! parser normalizes decimal, octal and hex IPv4 forms), checked in this
//! order: denied (never promptable), loopback (auto-granted except on
//! ports termul itself listens on), allow-listed (auto-granted), private
//! network (prompt), everything else (prompt once per session).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};

use parking_lot::{Mutex, RwLock};
use tauri::Url;

use super::ports::{ActionClass, CanonicalOrigin, OriginPattern};

/// Hostnames the Tauri runtime serves the app and its protocols on.
const APP_HOSTS: [&str; 3] = ["tauri.localhost", "ipc.localhost", "asset.localhost"];

const METADATA_HOSTS: [&str; 1] = ["metadata.google.internal"];
const METADATA_V4: Ipv4Addr = Ipv4Addr::new(169, 254, 169, 254);
const METADATA_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);

/// Every port a termul listener is bound to. Registered by the listeners
/// themselves at bind time; never removed, so a recycled port stays denied.
static OWNED_PORTS: RwLock<BTreeSet<u16>> = RwLock::new(BTreeSet::new());

/// Record a port termul listens on (web server, MCP host bridge,
/// annotation server, OAuth callback, the dev server).
pub fn register_owned_port(port: u16) {
    if port != 0 {
        OWNED_PORTS.write().insert(port);
    }
}

pub fn is_owned_port(port: u16) -> bool {
    OWNED_PORTS.read().contains(&port)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    Scheme,
    TermulOwned,
    AppOrigin,
    UnspecifiedAddress,
    MetadataAddress,
    LinkLocal,
}

impl DenyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DenyReason::Scheme => "scheme_not_allowed",
            DenyReason::TermulOwned => "termul_owned_origin",
            DenyReason::AppOrigin => "app_origin",
            DenyReason::UnspecifiedAddress => "unspecified_address",
            DenyReason::MetadataAddress => "metadata_address",
            DenyReason::LinkLocal => "link_local_address",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Unlisted,
    LocalNetwork,
}

impl PromptKind {
    pub fn action(self) -> ActionClass {
        match self {
            PromptKind::Unlisted => ActionClass::Navigate,
            PromptKind::LocalNetwork => ActionClass::LocalNetwork,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginClass {
    Loopback,
    AllowListed,
    Prompt(PromptKind),
    Denied(DenyReason),
}

enum HostKind {
    Loopback,
    LocalNetwork,
    Public,
    Denied(DenyReason),
}

fn classify_v4(ip: Ipv4Addr) -> HostKind {
    let [a, b, ..] = ip.octets();
    if ip.is_unspecified() {
        HostKind::Denied(DenyReason::UnspecifiedAddress)
    } else if ip == METADATA_V4 {
        HostKind::Denied(DenyReason::MetadataAddress)
    } else if ip.is_link_local() {
        HostKind::Denied(DenyReason::LinkLocal)
    } else if ip.is_loopback() {
        HostKind::Loopback
    } else if ip.is_private() || (a == 100 && (64..128).contains(&b)) {
        HostKind::LocalNetwork
    } else {
        HostKind::Public
    }
}

fn classify_v6(ip: Ipv6Addr) -> HostKind {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return classify_v4(v4);
    }
    let first = ip.segments()[0];
    if ip.is_unspecified() {
        HostKind::Denied(DenyReason::UnspecifiedAddress)
    } else if ip == METADATA_V6 {
        HostKind::Denied(DenyReason::MetadataAddress)
    } else if first & 0xffc0 == 0xfe80 {
        HostKind::Denied(DenyReason::LinkLocal)
    } else if ip.is_loopback() {
        HostKind::Loopback
    } else if first & 0xfe00 == 0xfc00 {
        HostKind::LocalNetwork
    } else {
        HostKind::Public
    }
}

fn classify_domain(domain: &str) -> HostKind {
    let domain = domain.strip_suffix('.').unwrap_or(domain);
    if APP_HOSTS.contains(&domain) {
        HostKind::Denied(DenyReason::AppOrigin)
    } else if METADATA_HOSTS.contains(&domain) {
        HostKind::Denied(DenyReason::MetadataAddress)
    } else if domain == "localhost" || domain.ends_with(".localhost") {
        HostKind::Loopback
    } else {
        HostKind::Public
    }
}

/// The origin class of `url`. `owned_port` says whether a loopback port
/// is one termul listens on.
pub fn classify(
    url: &Url,
    owned_port: impl Fn(u16) -> bool,
    allow_list: &[OriginPattern],
) -> OriginClass {
    if !matches!(url.scheme(), "http" | "https") {
        return OriginClass::Denied(DenyReason::Scheme);
    }
    let Some(host) = url.host_str() else {
        return OriginClass::Denied(DenyReason::Scheme);
    };
    // The parser already serialized IP hosts canonically: dotted quad, or
    // bracketed compressed IPv6.
    let kind = if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        match v6.parse::<Ipv6Addr>() {
            Ok(ip) => classify_v6(ip),
            Err(_) => return OriginClass::Denied(DenyReason::Scheme),
        }
    } else if let Ok(ip) = host.parse::<Ipv4Addr>() {
        classify_v4(ip)
    } else {
        classify_domain(host)
    };
    let listed = || {
        CanonicalOrigin::parse(url.as_str())
            .is_some_and(|origin| allow_list.iter().any(|p| p.matches(&origin)))
    };
    match kind {
        HostKind::Denied(reason) => OriginClass::Denied(reason),
        HostKind::Loopback => match url.port_or_known_default() {
            Some(port) if owned_port(port) => OriginClass::Denied(DenyReason::TermulOwned),
            _ => OriginClass::Loopback,
        },
        _ if listed() => OriginClass::AllowListed,
        HostKind::LocalNetwork => OriginClass::Prompt(PromptKind::LocalNetwork),
        HostKind::Public => OriginClass::Prompt(PromptKind::Unlisted),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HopVerdict {
    Allow,
    Deny(DenyReason),
    /// Promptable, but not granted for this session yet.
    Ungranted(PromptKind, CanonicalOrigin),
}

/// Fold a class and the session's grants into a verdict for one hop.
pub fn hop_verdict(
    url: &Url,
    class: OriginClass,
    granted: impl Fn(&CanonicalOrigin) -> bool,
) -> HopVerdict {
    match class {
        OriginClass::Loopback | OriginClass::AllowListed => HopVerdict::Allow,
        OriginClass::Denied(reason) => HopVerdict::Deny(reason),
        OriginClass::Prompt(kind) => match CanonicalOrigin::parse(url.as_str()) {
            Some(origin) if granted(&origin) => HopVerdict::Allow,
            Some(origin) => HopVerdict::Ungranted(kind, origin),
            None => HopVerdict::Deny(DenyReason::Scheme),
        },
    }
}

#[derive(Default)]
struct SessionAuthz {
    granted: HashSet<CanonicalOrigin>,
    /// Allow-list as of the session's last authorized call: what the
    /// synchronous navigation hook decides from.
    allow_list: Vec<OriginPattern>,
}

/// Per-session origin grants, keyed `(session, origin)`. They end at
/// session end and at Stop.
#[derive(Default)]
pub struct SessionGrants {
    sessions: Mutex<HashMap<String, SessionAuthz>>,
}

impl SessionGrants {
    pub fn grant(&self, session_id: &str, origin: CanonicalOrigin) {
        self.sessions
            .lock()
            .entry(session_id.to_string())
            .or_default()
            .granted
            .insert(origin);
    }

    pub fn revoke_session(&self, session_id: &str) {
        if let Some(session) = self.sessions.lock().get_mut(session_id) {
            session.granted.clear();
        }
    }

    pub fn forget_session(&self, session_id: &str) {
        self.sessions.lock().remove(session_id);
    }

    pub fn set_allow_list(&self, session_id: &str, allow_list: Vec<OriginPattern>) {
        self.sessions
            .lock()
            .entry(session_id.to_string())
            .or_default()
            .allow_list = allow_list;
    }

    /// The verdict for one navigation hop of a session-owned tab, from the
    /// grants and allow-list snapshot alone (no I/O: the engine hook that
    /// calls this cannot wait). `about:blank` is the engine's own empty
    /// document and carries no request.
    pub fn check_hop(&self, session_id: &str, url: &Url) -> HopVerdict {
        if url.as_str() == "about:blank" {
            return HopVerdict::Allow;
        }
        let sessions = self.sessions.lock();
        let session = sessions.get(session_id);
        let allow_list = session.map_or(&[][..], |s| s.allow_list.as_slice());
        let class = classify(url, is_owned_port, allow_list);
        hop_verdict(url, class, |origin| {
            session.is_some_and(|s| s.granted.contains(origin))
        })
    }
}

#[cfg(test)]
#[path = "authz_tests.rs"]
mod tests;
