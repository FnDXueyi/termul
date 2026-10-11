//! Working-tree diff stat (`+added -removed`) for CAP-1 project rows.
//!
//! One shared function — [`git_get_diff_stat`] — is the single source of truth
//! behind both the Tauri command and the `POST /git/diff-stat` web route.
//!
//! The stat matches git: tracked lines are the totals of
//! `git diff --numstat HEAD` (staged + unstaged; against the empty tree when
//! the repository has no HEAD yet), and untracked lines are counted from every
//! untracked, non-ignored regular file. Binary files contribute 0. The
//! untracked scan is bounded ([`MAX_UNTRACKED_FILES`] files,
//! [`MAX_UNTRACKED_FILE_BYTES`] per file) and never follows symlinks.
//!
//! File contents are never logged.

use super::*;

use std::io::Read;
use std::path::Path;

/// Object id of git's well-known empty tree; the diff base when there is no HEAD.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// Maximum untracked files scanned per call; the rest contribute 0.
pub(super) const MAX_UNTRACKED_FILES: usize = 5000;
/// Maximum size of a single untracked file we will read; larger files contribute 0.
pub(super) const MAX_UNTRACKED_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Generous per-command timeout: `diff` on a large worktree can outlast the
/// 2s status-poll default, which would fail the whole stat.
const DIFF_STAT_GIT_TIMEOUT_MS: u64 = 15_000;

/// Added / removed line totals for a working tree.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffStat {
    pub added: u64,
    pub removed: u64,
}

/// Why a diff stat could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffStatError {
    /// git could not be started, or it timed out.
    GitUnavailable(String),
    /// git ran and exited non-zero (for example, the cwd is not a repository).
    GitFailed {
        command: &'static str,
        detail: String,
    },
}

impl DiffStatError {
    /// Stable machine code shared by the Tauri `IpcResult` and the web `IpcBody`.
    pub fn code(&self) -> &'static str {
        "GIT_DIFF_STAT_ERROR"
    }
}

impl std::fmt::Display for DiffStatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffStatError::GitUnavailable(detail) => {
                write!(f, "Failed to run git for diff stat: {detail}")
            }
            DiffStatError::GitFailed { command, detail } if detail.is_empty() => {
                write!(f, "git {command} failed")
            }
            DiffStatError::GitFailed { command, detail } => {
                write!(f, "git {command} failed: {detail}")
            }
        }
    }
}

impl std::error::Error for DiffStatError {}

/// Compute the `+added -removed` stat for the repository containing `cwd`.
pub fn git_get_diff_stat(cwd: &str) -> Result<GitDiffStat, DiffStatError> {
    match compute_diff_stat(cwd) {
        Ok(stat) => {
            log::debug!(
                "[GitTracker] diff stat for {cwd}: +{} -{}",
                stat.added,
                stat.removed
            );
            Ok(stat)
        }
        Err(e) => {
            log::warn!("[GitTracker] diff stat failed for {cwd}: {e}");
            Err(e)
        }
    }
}

fn compute_diff_stat(cwd: &str) -> Result<GitDiffStat, DiffStatError> {
    // Also the "is this a repository" check: fails for a non-repo cwd.
    let toplevel = git_stdout(cwd, "rev-parse", &["rev-parse", "--show-toplevel"])?;
    let toplevel = String::from_utf8_lossy(&toplevel).trim().to_string();

    let base = if super::commands::repo_has_head(cwd) {
        "HEAD"
    } else {
        EMPTY_TREE
    };
    let numstat = git_stdout(cwd, "diff", &["diff", "--numstat", base])?;
    let (added, removed) = parse_numstat(&String::from_utf8_lossy(&numstat));

    // `--full-name` + the `:/` pathspec make the listing repo-wide, matching
    // the repo-wide scope of `git diff --numstat`.
    let untracked = git_stdout(
        cwd,
        "ls-files",
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--full-name",
            "-z",
            "--",
            ":/",
        ],
    )?;
    let untracked_added = count_untracked_lines(Path::new(&toplevel), &untracked);

    Ok(GitDiffStat {
        added: added.saturating_add(untracked_added),
        removed,
    })
}

/// Run git and return stdout, mapping spawn failure / timeout / non-zero exit
/// to a [`DiffStatError`].
fn git_stdout(cwd: &str, label: &'static str, args: &[&str]) -> Result<Vec<u8>, DiffStatError> {
    let output = run_git_capture(cwd, args, DIFF_STAT_GIT_TIMEOUT_MS)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(DiffStatError::GitFailed {
            command: label,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// Spawn git, drain stdout/stderr on helper threads (so large output cannot
/// fill the pipe and stall the child), and kill it after `timeout_ms`.
fn run_git_capture(
    cwd: &str,
    args: &[&str],
    timeout_ms: u64,
) -> Result<std::process::Output, DiffStatError> {
    let mut command = backend_command(resolve_git_binary());
    command
        .args(args)
        .current_dir(cwd)
        // Read-only stat: never take the index lock for an opportunistic refresh.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| DiffStatError::GitUnavailable(e.to_string()))?;

    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DiffStatError::GitUnavailable(format!(
                    "git {} timed out",
                    args.first().copied().unwrap_or("")
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(DiffStatError::GitUnavailable(e.to_string())),
        }
    };

    Ok(std::process::Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    })
}

/// Sum a `git diff --numstat` listing. Binary rows (`-\t-\t<path>`) and any
/// row that does not parse contribute 0.
pub(super) fn parse_numstat(output: &str) -> (u64, u64) {
    let mut added = 0u64;
    let mut removed = 0u64;
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(a), Some(r), Some(_path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if let (Ok(a), Ok(r)) = (a.parse::<u64>(), r.parse::<u64>()) {
            added = added.saturating_add(a);
            removed = removed.saturating_add(r);
        }
    }
    (added, removed)
}

/// Lines in `bytes`: the number of `\n`, plus one when the content is
/// non-empty and does not end with a newline.
pub(super) fn count_lines(bytes: &[u8]) -> u64 {
    let newlines = bytes.iter().filter(|b| **b == b'\n').count() as u64;
    match bytes.last() {
        Some(last) if *last != b'\n' => newlines + 1,
        _ => newlines,
    }
}

/// Count lines across the NUL-separated untracked listing (paths relative to
/// `toplevel`). Bounded by [`MAX_UNTRACKED_FILES`] / [`MAX_UNTRACKED_FILE_BYTES`];
/// symlinks, directories, unreadable, oversized and binary files add 0.
pub(super) fn count_untracked_lines(toplevel: &Path, listing: &[u8]) -> u64 {
    let mut total = 0u64;
    let mut skipped_oversize = 0usize;

    for (scanned, raw) in listing
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .enumerate()
    {
        if scanned >= MAX_UNTRACKED_FILES {
            log::debug!(
                "[GitTracker] diff stat: untracked scan capped at {MAX_UNTRACKED_FILES} files"
            );
            break;
        }

        let rel = String::from_utf8_lossy(raw);
        let path = toplevel.join(rel.as_ref());
        // `symlink_metadata` never follows links; only plain files count.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        if meta.len() > MAX_UNTRACKED_FILE_BYTES {
            skipped_oversize += 1;
            continue;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        // `take` bounds the read even if the file grows after the metadata check.
        let mut bytes = Vec::new();
        if file
            .take(MAX_UNTRACKED_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_UNTRACKED_FILE_BYTES
        {
            skipped_oversize += 1;
            continue;
        }
        if bytes.contains(&0) {
            continue;
        }
        total = total.saturating_add(count_lines(&bytes));
    }

    if skipped_oversize > 0 {
        log::debug!(
            "[GitTracker] diff stat: skipped {skipped_oversize} untracked file(s) over {MAX_UNTRACKED_FILE_BYTES} bytes"
        );
    }
    total
}
