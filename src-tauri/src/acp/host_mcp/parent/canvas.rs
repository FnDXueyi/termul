//! `Canvas` frames: resolve the session's project on every call and hand the
//! call to the canvas router (spine AD-21).

use super::HostPlanServer;
use crate::acp::host_mcp::canvas::{CanvasError, CanvasErrorCode};
use crate::acp::host_mcp::FrameReply;

impl HostPlanServer {
    pub(super) async fn canvas_call(
        &self,
        session_id: &str,
        method: Option<String>,
        params: Option<String>,
    ) -> FrameReply {
        let Some(method) = method else {
            return canvas_failure(&CanvasError::new(
                CanvasErrorCode::UnknownMethod,
                "a canvas frame needs a method",
            ));
        };
        let Some(project_id) = self.project_of(session_id) else {
            log::warn!(
                "[canvas-gateway] {method} refused: session {} has no project",
                crate::logging::redact_session_id(session_id)
            );
            return canvas_failure(&CanvasError::new(
                CanvasErrorCode::CanvasClosed,
                "this session belongs to no project, so it has no canvas",
            ));
        };
        let params = params.unwrap_or_else(|| "{}".to_string());
        match self.canvas.call(&project_id, &method, params).await {
            Ok(result) => FrameReply::ok_with(serde_json::Value::String(result)),
            Err(error) => canvas_failure(&error),
        }
    }

    /// The session's project, read from the session registry each time: a
    /// cached binding could outlive a project change.
    fn project_of(&self, session_id: &str) -> Option<String> {
        self.persistence
            .as_ref()?
            .metadata(session_id)
            .ok()?
            .project_id
            .filter(|project| !project.is_empty())
    }
}

fn canvas_failure(error: &CanvasError) -> FrameReply {
    FrameReply::err_code(error.code.as_str(), error.message.clone())
}
