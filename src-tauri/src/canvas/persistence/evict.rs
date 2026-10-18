//! The mount cap (spine AD-26): at most `canvas.max_mounted` canvases are
//! mounted, and admitting one more evicts the least recently active.
//!
//! Activity is a user looking at the canvas ([`CanvasPersistence::set_visible`])
//! or an agent calling it, so a hidden project an agent is working in is not
//! the victim. An eviction is an ordinary [`CanvasPersistence::close`]: the
//! AD-31 sequence saves first, and a victim that cannot be saved stays
//! mounted and blocks the new mount.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use super::{CanvasPersistence, CloseMode, Lifecycle};
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

/// What [`CanvasPersistence::admit`] did. `evicted` lists every canvas
/// unmounted along the way, including when `mount` is the refusal: evictions
/// that finished before a later victim failed to save are not undone.
#[derive(Debug)]
pub struct Admitted {
    pub evicted: Vec<Eviction>,
    pub mount: Result<u64, CanvasError>,
}

fn blocked(message: String) -> CanvasError {
    CanvasError::new(CODE_EVICTION_BLOCKED, message)
}

impl CanvasPersistence {
    /// Mount `doc_path` for `project_id`, first unmounting the least recently
    /// active other canvases until fewer than `cap` remain. The project's own
    /// previous mount is replaced, not counted.
    ///
    /// A victim that cannot be saved (or is already closing) stops the loop:
    /// it stays mounted and `mount` is `EVICTION_BLOCKED`.
    pub async fn admit(
        &self,
        project_id: &str,
        doc_path: PathBuf,
        autosave: Duration,
        cap: usize,
    ) -> Admitted {
        let _admitting = self.admission.lock().await;
        let mut evicted = Vec::new();
        loop {
            let victim = match self.victim(project_id, cap) {
                Ok(Some(victim)) => victim,
                Ok(None) => {
                    let mount_seq = self.mount(project_id, doc_path, autosave);
                    return Admitted {
                        evicted,
                        mount: Ok(mount_seq),
                    };
                }
                Err(error) => {
                    return Admitted {
                        evicted,
                        mount: Err(error),
                    }
                }
            };
            match self.close(&victim, CloseMode::Save).await {
                Ok(outcome) if outcome.unmounted => evicted.push(Eviction {
                    project_id: victim,
                    reason: REASON_CAP_REACHED,
                }),
                Ok(_) => {}
                Err(error) => {
                    return Admitted {
                        evicted,
                        mount: Err(blocked(format!(
                            "could not unload the least recently active canvas (project {victim}) \
                             to make room: {}",
                            error.message
                        ))),
                    }
                }
            }
        }
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
