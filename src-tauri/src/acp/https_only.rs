//! Redirect policy shared by host fetches of fixed https URLs: the ACP
//! registry snapshot and the agent archive download.

/// Follow https hops (GitHub releases and CDNs redirect to other https hosts)
/// under reqwest's default hop cap, and fail the request on a hop to any
/// other scheme so a downgrade never goes out over plaintext.
pub(crate) fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let scheme = attempt.url().scheme().to_string();
        if scheme == "https" {
            reqwest::redirect::Policy::default().redirect(attempt)
        } else {
            attempt.error(format!(
                "refused redirect to non-https URL scheme {scheme:?}"
            ))
        }
    })
}
