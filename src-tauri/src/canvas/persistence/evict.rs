//! The mount cap (spine AD-26): at most `canvas.max_mounted` canvases are
//! mounted, and admitting one more evicts the least recently active.
//!
//! Activity is a user looking at the canvas ([`CanvasPersistence::set_visible`])
//! or an agent calling it, so a hidden project an agent is working in is not
//! the victim. An eviction is an ordinary [`CanvasPersistence::close`]: the
//! AD-31 sequence saves first, and a victim that cannot be saved stays
//! mounted and blocks the new mount.

use serde::Serialize;

use super::{CanvasPersistence, CloseMode, Lifecycle, Opened};
use crate::canvas::CanvasError;

/// `canvas.max_mounted` when the renderer sends none.
pub const DEFAULT_MAX_MOUNTED: usize = 2;
const MAX_MAX_MOUNTED: usize = 16;

/// A canvas could not be unloaded to make room, so the new mount was refused.
pub const CODE_EVICTION_BLOCKED: &str = "EVICTION_BLOCKED";

const REASON_CAP_REACHED: &str = "mount cap reached";

/// The cap for a requested `canvas.max_mounted`, clamped to a sane range.
pub fn mount_cap(requested: Option<u64>) -> usize {
    requested.map_or(DEFAULT_MAX_MOUNTED, |cap| {
        usize::try_from(cap).map_or(MAX_MAX_MOUNTED, |cap| cap.clamp(1, MAX_MAX_MOUNTED))
    })
}

/// A canvas the cap unmounted after saving it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Eviction {
    pub project_id: String,
    pub reason: &'static str,
}

/// What [`CanvasPersistence::open`] did. `evicted` lists every canvas
/// unmounted along the way, including when `opened` is the refusal:
/// evictions that finished before a later victim failed to save are not
/// undone.
#[derive(Debug)]
pub struct Admitted {
    pub evicted: Vec<Eviction>,
    pub opened: Result<Opened, CanvasError>,
}

fn blocked(message: String) -> CanvasError {
    CanvasError::new(CODE_EVICTION_BLOCKED, message)
}

impl CanvasPersistence {
    /// Unmount the least recently active other canvases until fewer than
    /// `cap` remain. The project's own mount is replaced by the open, not
    /// counted. Call with the admission lock held, so two opens cannot both
    /// see room.
    ///
    /// A victim that cannot be saved (or is already closing) stops the loop:
    /// it stays mounted and the open is `EVICTION_BLOCKED`.
    pub(super) async fn make_room(
        &self,
        project_id: &str,
        cap: usize,
        evicted: &mut Vec<Eviction>,
    ) -> Result<(), CanvasError> {
        while let Some(victim) = self.victim(project_id, cap)? {
            match self.close(&victim, CloseMode::Save).await {
                Ok(outcome) if outcome.unmounted => evicted.push(Eviction {
                    project_id: victim,
                    reason: REASON_CAP_REACHED,
                }),
                Ok(_) => {}
                Err(error) => {
                    return Err(blocked(format!(
                        "could not unload the least recently active canvas (project {victim}) \
                         to make room: {}",
                        error.message
                    )))
                }
            }
        }
        Ok(())
    }

    /// The project to unload so one more canvas fits under `cap`: `None` when
    /// there is room. Only open mounts are candidates; if every other mount
    /// is already closing, nothing can be freed right now.
    fn victim(&self, project_id: &str, cap: usize) -> Result<Option<String>, CanvasError> {
        let others: Vec<_> = self
            .mounts
            .lock()
            .iter()
            .filter(|(project, _)| project.as_str() != project_id)
            .map(|(project, mount)| (project.clone(), mount.clone()))
            .collect();
        if others.len() < cap {
            return Ok(None);
        }
        others
            .iter()
            .filter_map(|(project, mount)| {
                let state = mount.state.lock();
                (state.lifecycle == Lifecycle::Open).then(|| (state.activity.rank(), project))
            })
            .min()
            .map(|(_, project)| Some(project.clone()))
            .ok_or_else(|| blocked("every other canvas is closing; try again".to_string()))
    }
}
