use super::*;
use crate::browser_automation::core::test_support::{InMemoryPolicyStore, ScriptedPermissions};

fn origin(s: &str) -> CanonicalOrigin {
    CanonicalOrigin::parse(s).unwrap_or_else(|| panic!("{s} must parse"))
}

#[test]
fn canonical_origin_lowercases_punycodes_and_elides_default_ports() {
    assert_eq!(origin("HTTPS://Example.COM/path?q=1").as_str(), "https://example.com");
    assert_eq!(origin("http://example.com:80").as_str(), "http://example.com");
    assert_eq!(origin("https://example.com:443/").as_str(), "https://example.com");
    assert_eq!(origin("http://localhost:5173").as_str(), "http://localhost:5173");
    assert_eq!(origin("https://bücher.example").as_str(), "https://xn--bcher-kva.example");
    assert_eq!(origin("http://[::1]:3000").as_str(), "http://[::1]:3000");
    for bad in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "tauri://localhost",
        "https://user:pw@example.com",
        "not a url",
    ] {
        assert_eq!(CanonicalOrigin::parse(bad), None, "{bad}");
    }
}

#[test]
fn origin_patterns_match_exact_origins_and_strict_subdomains_only() {
    let exact = OriginPattern::parse("https://example.com").expect("exact parses");
    assert!(exact.matches(&origin("https://example.com/a")));
    assert!(!exact.matches(&origin("https://a.example.com")));
    assert!(!exact.matches(&origin("http://example.com")));

    let subs = OriginPattern::parse("https://*.Example.com").expect("wildcard parses");
    assert!(subs.matches(&origin("https://a.example.com")));
    assert!(subs.matches(&origin("https://a.b.example.com")));
    assert!(!subs.matches(&origin("https://example.com")), "parent itself is not a subdomain");
    assert!(!subs.matches(&origin("https://evilexample.com")));
    assert!(!subs.matches(&origin("http://a.example.com")), "scheme is not wildcarded");
    assert!(!subs.matches(&origin("https://a.example.com:8443")), "port is not wildcarded");

    for bad in ["*", "https://*", "*://example.com", "https://*.*.example.com", "https://a.*.com", "ftp://*.example.com"] {
        assert_eq!(OriginPattern::parse(bad), None, "{bad}");
    }
}

#[test]
fn consent_options_map_to_acp_kinds_and_scope_markers() {
    let mapped: Vec<_> = ConsentOption::ALL
        .iter()
        .map(|o| (o.acp_kind(), o.scope()))
        .collect();
    assert_eq!(
        mapped,
        [
            ("allow_once", Some("session")),
            ("allow_always", Some("project")),
            ("reject_once", None),
        ]
    );
}

#[test]
fn ticket_ids_are_host_namespaced_uuids() {
    let id = TicketId::mint();
    assert!(id.as_str().starts_with("host:"));
    assert_eq!(TicketId::parse(id.as_str()), Some(id));
    for bad in ["host:", "host:nope", "q-123", "8f1c0f3e-8a51-4c4e-9a52-0d0f3b1f6a10"] {
        assert_eq!(TicketId::parse(bad), None, "{bad}");
    }
}

#[test]
fn policy_ops_are_idempotent() {
    let pattern = OriginPattern::parse("http://localhost:3000").unwrap();
    let pre = Preapproval {
        action: ActionClass::Upload,
        origin: origin("https://example.com"),
    };
    let once = ProjectPolicy::default()
        .apply(ProjectPolicyOp::AddOrigin(pattern.clone()))
        .apply(ProjectPolicyOp::SetPreapproval { preapproval: pre.clone(), allowed: true });
    let twice = once
        .clone()
        .apply(ProjectPolicyOp::AddOrigin(pattern.clone()))
        .apply(ProjectPolicyOp::SetPreapproval { preapproval: pre.clone(), allowed: true });
    assert_eq!(once, twice);
    assert_eq!(once.allow_list, vec![pattern.clone()]);

    let cleared = twice
        .apply(ProjectPolicyOp::RemoveOrigin(pattern.clone()))
        .apply(ProjectPolicyOp::RemoveOrigin(pattern))
        .apply(ProjectPolicyOp::SetPreapproval { preapproval: pre, allowed: false })
        .apply(ProjectPolicyOp::SetProfileMode(ProfileMode::Ephemeral));
    assert!(cleared.allow_list.is_empty() && cleared.preapprovals.is_empty());
    assert_eq!(cleared.profile_mode, ProfileMode::Ephemeral);

    let app = AppPolicy::default();
    assert!(app.global_profile_enabled, "the global profile defaults on");
    assert!(!app.apply(AppPolicyOp::SetGlobalProfileEnabled(false)).global_profile_enabled);
}

#[tokio::test]
async fn in_memory_store_enforces_revision_cas_and_read_only() {
    let store = InMemoryPolicyStore::default();
    let op = || ProjectPolicyOp::SetProfileMode(ProfileMode::Ephemeral);
    let first = store.apply_project("p1", 0, op()).await.expect("fresh write");
    assert_eq!(first.revision, 1);
    assert_eq!(
        store.apply_project("p1", 0, op()).await,
        Err(PolicyError::Conflict { current: 1 }),
        "a stale revision loses instead of overwriting"
    );
    assert_eq!(store.project("p1").await.unwrap(), first);
    assert_eq!(store.project("p2").await.unwrap(), ProjectPolicy::default());

    store.mark_project_read_only("p1");
    assert_eq!(store.apply_project("p1", 1, op()).await, Err(PolicyError::ReadOnly));

    let app = store
        .apply_app(0, AppPolicyOp::SetGlobalProfileEnabled(false))
        .await
        .expect("app write");
    assert_eq!((app.revision, app.global_profile_enabled), (1, false));
    store.mark_app_read_only();
    assert_eq!(
        store.apply_app(1, AppPolicyOp::SetGlobalProfileEnabled(true)).await,
        Err(PolicyError::ReadOnly)
    );
}

#[tokio::test]
async fn scripted_permissions_answer_in_order_then_default_to_unattended() {
    let perms = ScriptedPermissions::answering([TicketOutcome::Answered(ConsentOption::AllowSession)]);
    let ticket = || TicketRequest {
        id: TicketId::mint(),
        session_id: "s1".into(),
        project_id: "p1".into(),
        origin: origin("https://example.com"),
        target_url: "https://example.com/login".into(),
        action: ActionClass::Navigate,
        element: None,
        profile: TabProfile::Global,
    };
    let wait = Duration::from_secs(1);
    assert_eq!(
        perms.request(ticket(), wait).await,
        TicketOutcome::Answered(ConsentOption::AllowSession)
    );
    assert_eq!(perms.request(ticket(), wait).await, TicketOutcome::Unattended);
    assert_eq!(perms.requests.lock().len(), 2);
    perms.cancel_session("s1");
    assert_eq!(perms.cancelled_sessions.lock().as_slice(), ["s1".to_string()]);
}
