use std::process::Command;

/// Build a helper command that does not spawn a visible console window.
/// Without this, GUI/release builds on Windows flash a console window for every
/// short-lived helper (git.exe, where.exe, cmd.exe) — highly visible when
/// worktree status polling runs `git status` every 2s. See
/// [`crate::process_util::hide_console`].
pub(super) fn quiet_command(program: &str) -> Command {
    let mut command = Command::new(program);
    crate::process_util::hide_console(&mut command);
    command
}

/// Directories that should never be symlinked into worktrees.
const SYMLINK_EXCLUSION_LIST: &[&str] = &[
    ".git",
    ".termul",
    ".worktrees",
    ".claude",
    ".codex",
    ".opencode",
    ".pi",
    ".pi-lens",
    ".agents",
    ".auto-claude",
    ".vscode",
    ".idea",
    "_bmad",
    "_bmad-output",
    "_bmad-bkp",
];

/// Check if a directory name is in the hardcoded exclusion list.
pub(super) fn is_excluded_dir(dir_name: &str) -> bool {
    SYMLINK_EXCLUSION_LIST.iter().any(|excluded| {
        dir_name == *excluded || dir_name.starts_with(&format!("{}{}", *excluded, "/"))
    })
}
