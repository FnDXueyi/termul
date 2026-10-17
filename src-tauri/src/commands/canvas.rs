//! Canvas Tauri commands. `canvas_open` / `canvas_close` mount and unmount the
//! figcraft editor served by the loopback bundle listener; they mirror
//! `POST /canvas/open|close` in `web/canvas_api.rs` (same `IpcResult`
//! envelope, same typed codes) and share `canvas::static_server::open_canvas`.
//! `canvas_save` / `canvas_status` still drive the OpenPencil daemon pool until
//! it is removed.

use std::sync::Arc;

use tauri::State;

use super::{sanitize_log_field, validate_project_path, IpcResult};
use crate::canvas::mcp_proxy;
use crate::canvas::pool::CanvasDaemonPool;
use crate::canvas::static_server::{
    close_canvas, open_canvas, CanvasBundleBase, CanvasOpenInfo, StaticServer,
    CODE_BUNDLE_LISTENER_DOWN,
};
use crate::canvas::{CanvasStatus, CODE_DAEMON_DOWN};

const LISTENER_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// `canvas_open { docPath, projectId }` — the loopback bundle base plus a
/// minted `ct` token for the project's canvas. Typed `BUNDLE_LISTENER_DOWN`
/// while the listener is not bound.
#[tauri::command]
pub async fn canvas_open(
    doc_path: String,
    project_id: String,
    server: State<'_, Arc<StaticServer>>,
) -> Result<IpcResult<CanvasOpenInfo>, String> {
    let doc_key = validate_and_stringify!(&doc_path);
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
    match open_canvas(base, &project_id, doc_key) {
        Ok(info) => {
            log::info!(
                "[canvas] open project_id={log_project_id} canvas_id={}",
                info.canvas_id
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

/// `canvas_close { projectId }` — revoke the project's bundle token.
/// Idempotent.
#[tauri::command]
pub async fn canvas_close(project_id: String) -> Result<IpcResult<bool>, String> {
    log::info!(
        "[canvas] close project_id={}",
        sanitize_log_field(&project_id)
    );
    close_canvas(&project_id);
    Ok(IpcResult::success(true))
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
