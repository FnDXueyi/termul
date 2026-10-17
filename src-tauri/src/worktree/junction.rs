//! NTFS directory junctions for the worktree symlink fallback.

use super::quiet_command;
use std::path::Path;

/// Create a junction at `target` that points to the directory `source`.
pub(super) fn create_junction(source: &Path, target: &Path) -> Result<(), String> {
    let source_str = source.to_string_lossy().to_string();
    let target_str = target.to_string_lossy().to_string();
    let output = quiet_command("cmd")
        .args(["/C", "mklink", "/J", &target_str, &source_str])
        .output()
        .map_err(|e| format!("Failed to run mklink: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
