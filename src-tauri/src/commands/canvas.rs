//! Canvas Tauri commands. `canvas_open` / `canvas_close` mount and unmount the
//! figcraft editor served by the loopback bundle listener; they mirror
//! `POST /canvas/open|close` in `web/canvas_api.rs` (same `IpcResult`
//! envelope, same typed codes) and share `canvas::static_server::open_canvas`.
//! The `canvas_*document*` / `canvas_dirty_changed` / `canvas_write_snapshot`
//! commands are the desktop transport of `canvas::persistence` and mirror the
//! web routes of the same names. `canvas_save` / `canvas_status` still drive
//! the OpenPencil daemon pool until it is removed.

use std::path::Path;
use std::sync::Arc;

use tauri::State;

use super::{sanitize_log_field, validate_project_path, IpcResult};
use crate::canvas::mcp_proxy;
use crate::canvas::persistence::{
    self, canvas_mounts, CloseMode, CloseOutcome, ConflictChoice, DirtyOutcome, DiskOutcome,
    DocumentBytes, PersistenceView, Remount, ResolveOutcome, WriteOutcome,
};
use crate::canvas::pool::CanvasDaemonPool;
use crate::canvas::static_server::{
    close_canvas, open_canvas, CanvasBundleBase, CanvasOpenInfo, StaticServer,
    CODE_BUNDLE_LISTENER_DOWN,
};
use crate::canvas::{CanvasError, CanvasStatus, CODE_DAEMON_DOWN};

const LISTENER_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The canonical doc path. A document that does not exist yet (the first
/// open of a project's `design.figcraft`) is validated through its parent
/// directory; the first save creates it.
fn validate_canvas_doc_path(doc_path: &str) -> Result<String, String> {
    let path = Path::new(doc_path);
    let validated = if path.exists() {
        validate_project_path(doc_path)?
    } else {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(format!("Invalid canvas document path: {doc_path}"));
        };
        let parent = parent
            .to_str()
            .ok_or_else(|| "Path contains invalid UTF-8".to_string())?;
        validate_project_path(parent)?.join(name)
    };
    validated
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| "Path contains invalid UTF-8".to_string())
}

fn persistence_result<T>(
    operation: &str,
    project_id: &str,
    result: Result<T, CanvasError>,
) -> IpcResult<T> {
    match result {
        Ok(value) => IpcResult::success(value),
        Err(err) => {
            log::warn!(
                "[canvas] {operation} failed project_id={} code={}",
                sanitize_log_field(project_id),
                err.code
            );
            IpcResult::error(err.message, err.code)
        }
    }
}

/// `canvas_open { docPath, projectId, autosaveMs?, crashTimeouts? }` — the loopback bundle
/// base plus a minted `ct` token and a new `mountSeq` for the project's
/// canvas. Typed `BUNDLE_LISTENER_DOWN` while the listener is not bound.
#[tauri::command]
pub async fn canvas_open(
    doc_path: String,
    project_id: String,
    autosave_ms: Option<u64>,
    crash_timeouts: Option<u32>,
    server: State<'_, Arc<StaticServer>>,
) -> Result<IpcResult<CanvasOpenInfo>, String> {
    let doc_key = match validate_canvas_doc_path(&doc_path) {
        Ok(doc_key) => doc_key,
        Err(message) => return Ok(IpcResult::error(message, "PATH_VALIDATION_FAILED")),
    };
    let log_project_id = sanitize_log_field(&project_id);
    let Some(base) = server.wait_for_base(LISTENER_WAIT).await else {
        log::warn!(
            "[canvas] open failed project_id={log_project_id} code={CODE_BUNDLE_LISTENER_DOWN}"
        );
        return Ok(IpcResult::error(
            "canvas bundle listener is not running",
            CODE_BUNDLE_LISTENER_DOWN,
        ));
    };
    match open_canvas(base, &project_id, doc_key, autosave_ms, crash_timeouts) {
        Ok(info) => {
            log::info!(
                "[canvas] open project_id={log_project_id} canvas_id={} mount_seq={}",
                info.canvas_id,
                info.mount_seq
            );
            Ok(IpcResult::success(info))
        }
        Err(err) => {
            log::warn!(
                "[canvas] open failed project_id={log_project_id} code={}",
                err.code
            );
            Ok(IpcResult::error(err.message, err.code))
        }
    }
}

/// `canvas_close { projectId, discard? }` — unmount the project's canvas
/// through the AD-31 sequence (quiesce, save, verify, unmount). A failed save
/// answers `SAVE_FAILED` and keeps the mount; `discard: true` is the user's
/// explicit choice to drop unsaved changes. Idempotent.
#[tauri::command]
pub async fn canvas_close(
    project_id: String,
    discard: Option<bool>,
) -> Result<IpcResult<CloseOutcome>, String> {
    let mode = close_mode(discard);
    let log_project_id = sanitize_log_field(&project_id);
    log::info!("[canvas] close project_id={log_project_id} mode={mode:?}");
    Ok(match close_canvas(&project_id, mode).await {
        Ok(outcome) => {
            log_closed(&log_project_id, &outcome);
            IpcResult::success(outcome)
        }
        Err(err) => {
            log::warn!(
                "[canvas] close refused project_id={log_project_id} code={}: {}",
                err.code,
                err.message
            );
            IpcResult::error(err.message, err.code)
        }
    })
}

/// The close mode a transport's `discard` flag asks for.
pub(crate) fn close_mode(discard: Option<bool>) -> CloseMode {
    if discard == Some(true) {
        CloseMode::Discard
    } else {
        CloseMode::Save
    }
}

fn log_closed(log_project_id: &str, outcome: &CloseOutcome) {
    if outcome.discarded {
        log::warn!(
            "[canvas] closed project_id={log_project_id}: unsaved changes discarded by the user"
        );
    } else {
        log::info!(
            "[canvas] closed project_id={log_project_id} unmounted={} saved={}",
            outcome.unmounted,
            outcome.saved_version.as_deref().unwrap_or("none")
        );
    }
}

/// `canvas_read_document { projectId, mountSeq }` — the mounted document's
/// bytes as base64, `null` when the file does not exist yet.
#[tauri::command]
pub async fn canvas_read_document(
    project_id: String,
    mount_seq: u64,
) -> Result<IpcResult<DocumentBytes>, String> {
    let result = persistence::read_document(canvas_mounts(), &project_id, mount_seq).await;
    Ok(persistence_result("read document", &project_id, result))
}

/// `canvas_document_loaded { projectId, mountSeq, ack }` — the editor holds
/// the document; `ack` is the `host.openDocument` result text or `null` for
/// a new document.
#[tauri::command]
pub async fn canvas_document_loaded(
    project_id: String,
    mount_seq: u64,
    ack: Option<String>,
) -> Result<IpcResult<PersistenceView>, String> {
    let result =
        persistence::document_loaded(canvas_mounts(), &project_id, mount_seq, ack.as_deref());
    Ok(persistence_result("document loaded", &project_id, result))
}

/// `canvas_dirty_changed { projectId, mountSeq, dirty, contentVersion }` —
/// answers once the autosave debounce settles: `saveDue`, `idle`, or
/// `superseded` by a later notification.
#[tauri::command]
pub async fn canvas_dirty_changed(
    project_id: String,
    mount_seq: u64,
    dirty: bool,
    content_version: String,
) -> Result<IpcResult<DirtyOutcome>, String> {
    let result = persistence::dirty_changed(
        canvas_mounts(),
        &project_id,
        mount_seq,
        dirty,
        &content_version,
    )
    .await;
    Ok(persistence_result("dirty changed", &project_id, result))
}

/// `canvas_write_snapshot { projectId, mountSeq, snapshot }` — write the
/// `host.snapshot` result text to the project's document atomically.
#[tauri::command]
pub async fn canvas_write_snapshot(
    project_id: String,
    mount_seq: u64,
    snapshot: String,
) -> Result<IpcResult<WriteOutcome>, String> {
    let result =
        persistence::write_snapshot(canvas_mounts(), &project_id, mount_seq, &snapshot).await;
    if let Ok(outcome) = &result {
        log_write_outcome(&project_id, outcome);
    }
    Ok(persistence_result("write snapshot", &project_id, result))
}

fn log_write_outcome(project_id: &str, outcome: &WriteOutcome) {
    let project_id = sanitize_log_field(project_id);
    match (&outcome.mark_saved, &outcome.view.save_error) {
        (Some(version), _) => {
            log::info!("[canvas] saved project_id={project_id} content_version={version}");
        }
        (None, Some(error)) => {
            log::warn!("[canvas] save failed project_id={project_id}: {error}");
        }
        (None, None) => {}
    }
}

/// `canvas_disk_changed { projectId, mountSeq }` — Termul's file watcher saw
/// the mounted document change: `unchanged` (Termul's own write), `reload`
/// with the bytes for a clean editor, or `conflict` (AD-27).
#[tauri::command]
pub async fn canvas_disk_changed(
    project_id: String,
    mount_seq: u64,
) -> Result<IpcResult<DiskOutcome>, String> {
    let result = canvas_mounts().disk_changed(&project_id, mount_seq).await;
    if let Ok(outcome) = &result {
        log_disk_outcome(&project_id, outcome);
    }
    Ok(persistence_result("disk changed", &project_id, result))
}

/// The one log line per external change both transports write.
pub(crate) fn log_disk_outcome(project_id: &str, outcome: &DiskOutcome) {
    let project_id = sanitize_log_field(project_id);
    match outcome {
        DiskOutcome::Unchanged { .. } => {}
        DiskOutcome::Reload { .. } => {
            log::info!("[canvas] reloaded from disk project_id={project_id}: changed outside Termul");
        }
        DiskOutcome::Conflict { view } => log::warn!(
            "[canvas] conflict project_id={project_id}: changed on disk with unsaved edits deleted={}",
            view.conflict
                .as_ref()
                .is_some_and(|conflict| conflict.disk_hash.is_none())
        ),
    }
}

/// `canvas_resolve_conflict { projectId, mountSeq, choice, snapshot? }` —
/// the user's answer to a disk conflict; `snapshot` is the `host.snapshot`
/// result text for `saveCopy`.
#[tauri::command]
pub async fn canvas_resolve_conflict(
    project_id: String,
    mount_seq: u64,
    choice: ConflictChoice,
    snapshot: Option<String>,
) -> Result<IpcResult<ResolveOutcome>, String> {
    log::info!(
        "[canvas] conflict resolved project_id={} choice={choice:?}",
        sanitize_log_field(&project_id)
    );
    let result = persistence::resolve_conflict(
        canvas_mounts(),
        &project_id,
        mount_seq,
        choice,
        snapshot.as_deref(),
    )
    .await;
    Ok(persistence_result("resolve conflict", &project_id, result))
}

/// `canvas_crashed { projectId, mountSeq, reason }` — the editor reported
/// `event.crashed`: remount from the last saved bytes under a new
/// `mountSeq` (AD-31).
#[tauri::command]
pub async fn canvas_crashed(
    project_id: String,
    mount_seq: u64,
    reason: String,
) -> Result<IpcResult<Remount>, String> {
    let result = canvas_mounts().crashed(&project_id, mount_seq, &reason);
    Ok(persistence_result("crashed", &project_id, result))
}

/// `canvas_save { docPath }` — save through the daemon (`POST
/// /api/file/save`); the daemon is the `.op` document authority. No live
/// daemon → typed `DAEMON_DOWN` (the "canvas closed" signal).
#[tauri::command]
pub async fn canvas_save(
    doc_path: String,
    pool: State<'_, Arc<CanvasDaemonPool>>,
) -> Result<IpcResult<serde_json::Value>, String> {
    let validated = validate_and_stringify!(&doc_path);
    let Some(daemon) = pool.daemon_for_doc(&validated) else {
        log::warn!("[canvas] save with no live daemon");
        return Ok(IpcResult::error(
            "canvas daemon is not running",
            CODE_DAEMON_DOWN,
        ));
    };
    match mcp_proxy::daemon_save(daemon.as_ref()).await {
        Ok(value) => {
            log::info!("[canvas] save dispatched");
            Ok(IpcResult::success(value))
        }
        Err(err) => {
            log::warn!("[canvas] save failed code={}", err.code);
            Ok(IpcResult::error(err.message, err.code))
        }
    }
}

/// `canvas_status` — pool snapshot (live daemons + active doc key).
#[tauri::command]
pub async fn canvas_status(
    pool: State<'_, Arc<CanvasDaemonPool>>,
) -> Result<IpcResult<CanvasStatus>, String> {
    Ok(IpcResult::success(pool.status()))
}

/// `canvas_bundle_base` — the loopback base URL the figcraft bundle loads
/// from plus the bundle tag this build pins. Waits briefly for the listener,
/// which binds asynchronously at startup; typed `BUNDLE_LISTENER_DOWN` when
/// it is down. The web twin is `GET /canvas/bundle-base` (same shape, empty
/// same-origin base).
#[tauri::command]
pub async fn canvas_bundle_base(
    server: State<'_, Arc<StaticServer>>,
) -> Result<IpcResult<CanvasBundleBase>, String> {
    match server
        .wait_for_base(std::time::Duration::from_secs(5))
        .await
    {
        Some(base) => Ok(IpcResult::success(base)),
        None => {
            log::warn!("[canvas] bundle base requested while the listener is down");
            Ok(IpcResult::error(
                "canvas bundle listener is not running",
                CODE_BUNDLE_LISTENER_DOWN,
            ))
        }
    }
}
