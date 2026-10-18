//! Every time budget of a browser tool call, and the absolute deadline the
//! parent stamps on each call frame.
//!
//! The child gives up after [`ROUND_TRIP`] (served to it in the describe
//! reply). The parent stamps a deadline [`MARGIN`] earlier, so the host
//! always answers before the child stops listening; the core refuses an
//! action whose budget no longer fits instead of running it late.

use std::time::Duration;

use tokio::time::Instant;

const fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

/// How long the child waits for the parent's reply to a browser call.
pub const ROUND_TRIP: Duration = secs(200);
/// Headroom between the host's deadline and the child giving up.
pub const MARGIN: Duration = secs(5);
/// The deadline the parent stamps on a browser frame when it arrives.
pub const FRAME_BUDGET: Duration = ROUND_TRIP.saturating_sub(MARGIN);
/// How long the child waits for the describe reply at startup.
pub const DESCRIBE: Duration = secs(5);

/// How long a consent prompt may wait for the user.
pub const CONSENT_WAIT: Duration = secs(120);
/// Renderer round trip to mount an agent tab's pane and webview.
pub const TAB_OPEN: Duration = secs(15);
/// Post-navigation wait for the document to become interactive.
pub const NAV_SETTLE: Duration = secs(15);
pub const SETTLE_POLL: Duration = Duration::from_millis(200);
/// Longest `wait`, by `ms` or for `text`.
pub const WAIT_MAX: Duration = secs(30);
pub const WAIT_POLL: Duration = Duration::from_millis(300);

/// WebView2: how long the UI thread may take to pick up a CDP command.
pub const CDP_ISSUE: Duration = secs(5);
/// WebView2: how long an issued CDP command may take to answer.
pub const CDP_RESULT: Duration = secs(15);
/// One CDP command, issue plus result.
pub const CDP_CALL: Duration = CDP_ISSUE.saturating_add(CDP_RESULT);
/// JS bridge: how long a page may take to post an eval result back.
pub const BRIDGE_EVAL: Duration = secs(20);
/// One helper-script round trip on any engine: on WebView2 an undelivered
/// CDP command falls back to the bridge once.
pub const EVAL_STEP: Duration = CDP_ISSUE.saturating_add(BRIDGE_EVAL);

/// Per-verb action budgets, referenced by the action registry. Each is the
/// worst case of the steps the verb runs.
pub const ACTION_INSTANT: Duration = secs(1);
pub const ACTION_EVAL: Duration = EVAL_STEP;
/// Resolve the ref, then press and release. The synthetic fallback runs
/// only if its own step still fits the deadline.
pub const ACTION_CLICK: Duration = EVAL_STEP.saturating_add(CDP_CALL).saturating_add(CDP_CALL);
/// A settle loop can start its last readiness probe just before it ends.
pub const ACTION_HISTORY: Duration = NAV_SETTLE.saturating_add(EVAL_STEP);
pub const ACTION_NAVIGATE: Duration = TAB_OPEN.saturating_add(ACTION_HISTORY);
pub const ACTION_NEW_TAB: Duration = TAB_OPEN;
pub const ACTION_WAIT: Duration = WAIT_MAX.saturating_add(EVAL_STEP);

/// An absolute point in time a call must finish by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(budget: Duration) -> Self {
        Self(Instant::now() + budget)
    }

    pub fn remaining(self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    /// True when `budget` still fits before the deadline.
    pub fn admits(self, budget: Duration) -> bool {
        self.remaining() >= budget
    }

    /// `step`, cut short to what is left.
    pub fn clamp(self, step: Duration) -> Duration {
        step.min(self.remaining())
    }
}

#[cfg(test)]
#[path = "budgets_tests.rs"]
mod tests;
