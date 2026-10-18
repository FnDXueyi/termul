//! Host classification of sensitive agent actions.
//!
//! The page helper reports closed [`SensitivityFacts`] about an action's
//! target; [`classify`] turns them, with what the host knows about the tab
//! and the session, into the sensitive classes the action needs consent
//! for. Classification is pure and monotonic: a fact can only add classes,
//! and a fact the helper could not establish counts as the sensitive case.
//! The agent's stated intent is never an input.

use std::collections::HashSet;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;
use tauri::Url;

use super::ports::{ActionClass, CanonicalOrigin};

/// What the sensitivity helper observed about an action's target element.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SensitivityFacts {
    /// Origin of the element's document.
    pub frame_origin: Option<String>,
    /// The element belongs to a form.
    pub target_in_form: bool,
    /// The form has a password field.
    pub form_has_password: bool,
    /// The form has a payment-card field.
    pub form_has_payment: bool,
    /// Activating the element submits its form.
    pub is_submitter: bool,
    pub is_file_input: bool,
    /// Absolute URL the form submits to.
    pub form_action: Option<String>,
    /// Absolute URL of the link the element sits in.
    pub link_target: Option<String>,
}

impl SensitivityFacts {
    /// The helper's reply as facts. A reply of any other shape is
    /// ambiguous, which classifies as a password and payment submit.
    pub fn from_helper(out: &Value) -> Self {
        serde_json::from_value(out.clone()).unwrap_or_else(|_| Self::ambiguous())
    }

    fn ambiguous() -> Self {
        Self {
            target_in_form: true,
            form_has_password: true,
            form_has_payment: true,
            is_submitter: true,
            ..Self::default()
        }
    }
}

/// The agent action being classified.
#[derive(Debug, Clone, Copy)]
pub enum Gesture<'a> {
    Click,
    Key(&'a str),
    Type(&'a str),
    Fill,
    Navigate(&'a Url),
    Upload,
}

/// What the host knows about the tab and session an action runs in.
#[derive(Debug, Clone, Copy)]
pub struct Scene<'a> {
    pub tab_url: &'a str,
    /// The session has read page content (a snapshot or screenshot).
    pub tainted: bool,
    /// The facts came from an isolated world page script cannot reach.
    pub facts_trusted: bool,
    /// The tab's origin is loopback or allow-listed.
    pub trusted_origin: bool,
}

/// One class an action needs consent for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sensitive {
    pub class: ActionClass,
    /// The origin a pre-approval is keyed on. `None` when no http(s)
    /// origin could be established: such an action is refused.
    pub origin: Option<CanonicalOrigin>,
    /// The URL the consent ticket shows.
    pub target: String,
    /// Page-controlled facts classified it: it always prompts and is never
    /// pre-approvable.
    pub pinned: bool,
}

fn origin_of(url: &str) -> Option<CanonicalOrigin> {
    CanonicalOrigin::parse(url).ok()
}

fn is_enter(key: &str) -> bool {
    ["enter", "numpadenter", "return", "\n", "\r"]
        .iter()
        .any(|k| key.eq_ignore_ascii_case(k))
}

fn has_newline(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// A URL carries data when it has a query or a fragment. An unparseable
/// one is assumed to.
fn carries_data(url: &str) -> bool {
    url.parse::<Url>().map_or(true, |u| {
        u.query().is_some_and(|q| !q.is_empty()) || u.fragment().is_some_and(|f| !f.is_empty())
    })
}

/// Whether `url` goes to another origin than the tab's. Unknown on either
/// side counts as leaving.
fn leaves(url: Option<&str>, tab: Option<&CanonicalOrigin>) -> bool {
    match (url.and_then(origin_of), tab) {
        (Some(to), Some(from)) => &to != from,
        _ => true,
    }
}

struct Classes(Vec<Sensitive>);

impl Classes {
    fn add(
        &mut self,
        class: ActionClass,
        origin: Option<CanonicalOrigin>,
        target: &str,
        pinned: bool,
    ) {
        if let Some(seen) = self
            .0
            .iter_mut()
            .find(|s| s.class == class && s.origin == origin)
        {
            seen.pinned |= pinned;
            return;
        }
        self.0.push(Sensitive {
            class,
            origin,
            target: target.to_string(),
            pinned,
        });
    }
}

/// Every sensitive class `gesture` falls in.
pub fn classify(
    gesture: Gesture<'_>,
    facts: &SensitivityFacts,
    scene: &Scene<'_>,
) -> Vec<Sensitive> {
    let tab_origin = origin_of(scene.tab_url);
    let page_origin = facts
        .frame_origin
        .as_deref()
        .and_then(origin_of)
        .or_else(|| tab_origin.clone());
    let page = scene.tab_url;
    let mut out = Classes(Vec::new());

    let submits = match gesture {
        Gesture::Click => facts.is_submitter && facts.target_in_form,
        Gesture::Key(key) => is_enter(key) && facts.target_in_form,
        Gesture::Type(text) => has_newline(text) && facts.target_in_form,
        Gesture::Fill | Gesture::Navigate(_) | Gesture::Upload => false,
    };
    if submits {
        if facts.form_has_password {
            out.add(
                ActionClass::PasswordSubmit,
                page_origin.clone(),
                page,
                false,
            );
        }
        if facts.form_has_payment {
            out.add(ActionClass::PaymentSubmit, page_origin.clone(), page, false);
        }
        let action = facts.form_action.as_deref();
        if scene.tainted && leaves(action, tab_origin.as_ref()) {
            let target = action.unwrap_or(page);
            let origin = action.and_then(origin_of).or_else(|| page_origin.clone());
            out.add(ActionClass::TaintedExfil, origin, target, false);
        }
    }

    match gesture {
        Gesture::Click => {
            if let Some(link) = facts.link_target.as_deref() {
                if scene.tainted && carries_data(link) && leaves(Some(link), tab_origin.as_ref()) {
                    out.add(ActionClass::TaintedExfil, origin_of(link), link, false);
                }
            }
        }
        Gesture::Navigate(url) => {
            let url = url.as_str();
            if scene.tainted && carries_data(url) && leaves(Some(url), tab_origin.as_ref()) {
                out.add(ActionClass::TaintedExfil, origin_of(url), url, false);
            }
        }
        Gesture::Upload => out.add(ActionClass::Upload, page_origin.clone(), page, false),
        Gesture::Key(_) | Gesture::Type(_) | Gesture::Fill => {}
    }

    // Facts the page can forge cannot clear a submit-shaped action on an
    // origin the user has not vouched for: it is treated as a credential
    // submit that always prompts.
    if !scene.facts_trusted && !scene.trusted_origin {
        let submit_shaped = match gesture {
            Gesture::Click => facts.is_submitter || facts.target_in_form,
            Gesture::Key(key) => is_enter(key),
            Gesture::Type(text) => has_newline(text) || facts.target_in_form,
            Gesture::Fill => facts.target_in_form,
            Gesture::Navigate(_) | Gesture::Upload => false,
        };
        if submit_shaped {
            out.add(ActionClass::PasswordSubmit, page_origin, page, true);
        }
    }
    out.0
}

/// Sessions whose agent has read page content. Untrusted content can carry
/// instructions, so from then on data-carrying requests to other origins
/// are exfiltration-shaped. A session never becomes untainted.
#[derive(Default)]
pub struct Taint {
    sessions: Mutex<HashSet<String>>,
}

impl Taint {
    pub fn mark(&self, session_id: &str) {
        self.sessions.lock().insert(session_id.to_string());
    }

    pub fn is_tainted(&self, session_id: &str) -> bool {
        self.sessions.lock().contains(session_id)
    }

    pub fn forget(&self, session_id: &str) {
        self.sessions.lock().remove(session_id);
    }
}

#[cfg(test)]
#[path = "sensitivity_tests.rs"]
mod tests;
