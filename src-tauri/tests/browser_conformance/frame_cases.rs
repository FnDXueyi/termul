//! Cases for content the main document does not hold itself: same- and
//! cross-origin frames, open and closed shadow roots, and a page whose
//! Content-Security-Policy forbids every script.

use std::time::{Duration, Instant};

use serde_json::json;

use super::{expect_code, find_ref, mismatch, CaseResult, Ctx, Failure};

const FRAMES_READY: &str = "Frames ready";
const SHADOW_READY: &str = "Shadow fixture ready";
const CSP_READY: &str = "CSP fixture ready";
/// The outline marker for a frame the engine cannot read.
const UNREACHABLE: &str = "(cross-origin frame, not readable on this engine)";
const FRAME_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(250);

impl Ctx<'_> {
    /// Snapshot until `line` shows (frames load after their parent), or
    /// the wait runs out. Returns the last outline.
    async fn outline_with(&self, line: &str) -> Result<String, Failure> {
        let until = Instant::now() + FRAME_WAIT;
        loop {
            let text = self.snapshot().await?.text;
            if text.contains(line) || Instant::now() > until {
                return Ok(text);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Fill the inner field, press the inner button, and expect the frame
    /// to report both.
    async fn act_inside(&self, field: (&str, &str), button: (&str, &str), done: &str) -> CaseResult {
        let outline = self.outline_with(&format!("- {} \"{}\" @e", button.0, button.1)).await?;
        let field_ref = find_ref(&outline, field.0, field.1)
            .ok_or_else(|| mismatch(format!("no {} \"{}\" ref in {outline}", field.0, field.1)))?;
        self.call("fill", json!({ "ref": field_ref, "value": "Ada" })).await?;
        self.click(button.0, button.1).await?;
        let after = self.outline_with(done).await?;
        if after.contains(done) {
            Ok(())
        } else {
            Err(mismatch(format!("after fill and click the outline lacks `{done}`: {after}")))
        }
    }

    /// Reload the frame from its parent, then expect a ref minted before
    /// the reload to be stale.
    async fn stale_after_frame_reload(&self, button: &str) -> CaseResult {
        let outline = self.snapshot().await?.text;
        let old = find_ref(&outline, "button", button).ok_or_else(|| mismatch(format!("no {button} ref")))?;
        self.click("button", "Reload frame").await?;
        let until = Instant::now() + FRAME_WAIT;
        loop {
            let attempt = self.call("click", json!({ "ref": old })).await;
            match attempt {
                Err(f) if f.code == "stale_ref" => return Ok(()),
                _ if Instant::now() > until => {
                    return expect_code(attempt, "stale_ref", "click on a ref from before the frame reloaded");
                }
                _ => tokio::time::sleep(POLL).await,
            }
        }
    }
}

pub async fn iframe_same_origin(c: &Ctx<'_>) -> CaseResult {
    c.open("/frames-same.html", FRAMES_READY).await?;
    c.act_inside(("textbox", "Inner field"), ("button", "Inner button"), "Inner result: clicked [Ada]")
        .await?;
    let outline = c.snapshot().await?.text;
    if !outline.contains("- iframe \"Same frame\"") {
        return Err(mismatch(format!("the outline does not show the frame: {outline}")));
    }
    c.stale_after_frame_reload("Inner button").await
}

/// Engines that cannot reach a cross-origin frame must say so in the
/// outline and mint no ref inside it; that is reported as unavailable.
pub async fn iframe_cross_origin(c: &Ctx<'_>) -> CaseResult {
    let path = format!("/frames-cross.html?xo={}", c.cross.port());
    c.open(&path, FRAMES_READY).await?;
    let outline = c.outline_with("Inner button").await?;
    if outline.contains(UNREACHABLE) && !outline.contains("Inner button") {
        return Err(Failure {
            code: "capability_unavailable".into(),
            reason: Some("cross_origin_frame".into()),
            message: "this engine cannot read cross-origin frames".into(),
        });
    }
    c.act_inside(("textbox", "Inner field"), ("button", "Inner button"), "Inner result: clicked [Ada]")
        .await?;
    c.stale_after_frame_reload("Inner button").await
}

pub async fn shadow_dom(c: &Ctx<'_>) -> CaseResult {
    c.open("/shadow.html", SHADOW_READY).await?;
    let outline = c.snapshot().await?.text;
    for line in ["- button \"Slotted button\" @e", "- textbox \"Shadow field\" @e"] {
        if !outline.contains(line) {
            return Err(mismatch(format!("the outline lacks `{line}`: {outline}")));
        }
    }
    if outline.contains("Closed button") {
        return Err(mismatch(format!("a closed shadow root was read: {outline}")));
    }
    c.act_inside(("textbox", "Shadow field"), ("button", "Shadow button"), "Shadow result: clicked [Ada]")
        .await
}

pub async fn csp_snapshot(c: &Ctx<'_>) -> CaseResult {
    c.open("/csp.html", CSP_READY).await?;
    let snap = c.snapshot().await?;
    if snap.title != "CSP fixture" {
        return Err(mismatch(format!("the page policy let a script run (title {})", snap.title)));
    }
    c.click("button", "CSP submit").await?;
    c.call("wait", json!({ "text": "Form done" })).await?;
    Ok(())
}
