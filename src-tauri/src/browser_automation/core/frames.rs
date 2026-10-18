//! Frames the main frame's helpers cannot enter. Same-origin frames and
//! open shadow roots need nothing here: the helpers walk into them and
//! their paths cross the boundary. A cross-origin frame shows in the main
//! outline as a frame item; where the engine reaches the frame on its own,
//! the host runs the snapshot helper there, splices its outline in, and
//! routes every helper for a ref minted from it to that frame.

use serde::Deserialize;
use serde_json::Value;

use super::events::label_prefix;
use super::reply::{engine_error, parse_snapshot, PageSnapshot, SnapshotItem};
use super::tabs::RefNode;
use super::{BrowserError, DesktopBrowserHost};
use crate::browser_automation::engine::assets::{self, Script};
use crate::browser_automation::engine::{ChildFrame, EngineError, Point};

/// Frames nested in frames the host follows; deeper ones stay unreachable.
const MAX_FRAME_DEPTH: usize = 4;
/// Outline items across the page and every spliced frame.
const MAX_ITEMS: usize = 1600;

/// What is left of the outline's item cap while frames are spliced.
struct Room {
    left: usize,
    capped: bool,
}

#[derive(Deserialize)]
struct Origin {
    x: f64,
    y: f64,
}

impl DesktopBrowserHost {
    async fn eval_frame(&self, label: &str, frame: Option<&str>, script: Script) -> Result<Value, BrowserError> {
        let Some(frame) = frame else {
            return self.eval(label, script).await;
        };
        self.engine.evaluate_in_frame(label, frame, script).await.map_err(|e: EngineError| {
            log::warn!("[browser-agent] frame eval failed on tab {} ({:?})", label_prefix(label), e.kind);
            engine_error(e, label)
        })
    }

    /// Run a helper for `node`'s element in the frame its ref was minted in.
    pub(super) async fn eval_ref(&self, label: &str, node: &RefNode, script: Script) -> Result<Value, BrowserError> {
        let frame = (!node.frame.is_empty()).then_some(node.frame.as_str());
        self.eval_frame(label, frame, script).await
    }

    /// The page outline with every reachable cross-origin frame spliced in.
    pub(super) async fn snapshot_page(&self, label: &str) -> Result<PageSnapshot, BrowserError> {
        let mut page = parse_snapshot(self.eval(label, assets::snapshot()).await?)?;
        if !page.items.iter().any(|i| matches!(i, SnapshotItem::Frame { .. })) {
            return Ok(page);
        }
        // Without frame access the frames stay in the outline, unreachable.
        let frames = self.engine.child_frames(label).await.unwrap_or_default();
        let mut room = Room {
            left: MAX_ITEMS.saturating_sub(page.items.len()),
            capped: page.capped,
        };
        let items = std::mem::take(&mut page.items);
        page.items = self.splice(label, &frames, None, items, 0, &mut room).await;
        page.capped = room.capped;
        Ok(page)
    }

    /// Replace each frame item of `parent`'s outline with the frame's own
    /// outline, one level deeper, when the engine reaches the frame.
    async fn splice(
        &self,
        label: &str,
        frames: &[ChildFrame],
        parent: Option<&str>,
        items: Vec<SnapshotItem>,
        nesting: usize,
        room: &mut Room,
    ) -> Vec<SnapshotItem> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let SnapshotItem::Frame { depth, name, path } = &item else {
                out.push(item);
                continue;
            };
            let child = frames
                .iter()
                .find(|f| f.parent.as_deref() == parent && f.owner_path == *path)
                .filter(|_| nesting < MAX_FRAME_DEPTH);
            let inner = match child {
                Some(child) => self.frame_outline(label, child).await,
                None => None,
            };
            let (Some(child), Some(inner)) = (child, inner) else {
                out.push(item);
                continue;
            };
            let mut inner_items = inner.items;
            let found = inner_items.len();
            inner_items.truncate(room.left);
            room.capped |= inner.capped || inner_items.len() < found;
            room.left -= inner_items.len();
            for nested in &mut inner_items {
                *nested.depth_mut() += depth + 1;
                if let SnapshotItem::Node { frame, .. } = nested {
                    frame.clone_from(&child.id);
                }
            }
            out.push(SnapshotItem::Landmark {
                depth: *depth,
                role: "iframe".to_string(),
                name: name.clone(),
            });
            let spliced = Box::pin(self.splice(label, frames, Some(&child.id), inner_items, nesting + 1, room));
            out.extend(spliced.await);
        }
        out
    }

    async fn frame_outline(&self, label: &str, frame: &ChildFrame) -> Option<PageSnapshot> {
        let out = self.eval_frame(label, Some(&frame.id), assets::frame_snapshot()).await.ok()?;
        parse_snapshot(out).ok()
    }

    /// Where `frame`'s viewport starts in main-frame CSS px: its frame
    /// element's content box in its parent, summed up to the page.
    pub(super) async fn frame_origin(&self, label: &str, frame: &str) -> Result<Point, BrowserError> {
        let gone = || {
            BrowserError::new(super::ERR_STALE_REF, "the frame holding the element is gone; take a new snapshot")
                .with_reason("frame_gone")
        };
        let frames = self.engine.child_frames(label).await.map_err(|e| engine_error(e, label))?;
        let mut at = Point { x: 0.0, y: 0.0 };
        let mut current = frame;
        for _ in 0..=MAX_FRAME_DEPTH {
            let child = frames.iter().find(|f| f.id == current).ok_or_else(gone)?;
            let out = self
                .eval_frame(label, child.parent.as_deref(), assets::frame_origin(&child.owner_path))
                .await?;
            let origin: Origin = serde_json::from_value(out).map_err(|_| gone())?;
            at.x += origin.x;
            at.y += origin.y;
            match &child.parent {
                None => return Ok(at),
                Some(parent) => current = parent,
            }
        }
        Err(gone())
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
pub(super) mod tests;
