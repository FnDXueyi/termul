//! Commit detail for the Git History view: the full message, the author email
//! and the changed files of one commit, with `+added -removed` per file.
//!
//! One shared function — [`git_get_commit_detail`] — backs both the Tauri
//! command and the `POST /git/commit-detail` web route.
//!
//! The files are the diff against the commit's first parent (the empty tree
//! for a root commit), with rename detection, the same view a merge commit
//! shows in `git log --first-parent -p`. The list is capped at
//! [`MAX_COMMIT_FILES`] entries; `files_truncated` tells the caller. The hash
//! is validated as hex before it reaches git, so it can never be read as an
//! option. File contents are never read or logged.

use super::*;

/// Object id of git's well-known empty tree; the diff base of a root commit.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// Maximum changed files returned for one commit; the rest are dropped.
pub(super) const MAX_COMMIT_FILES: usize = 1000;
/// `show` and `diff-tree` on a large commit can outlast the 2s poll default.
const COMMIT_DETAIL_GIT_TIMEOUT_MS: u64 = 15_000;
/// Field and record separators of the `git show` format below.
const FIELD_SEP: char = '\0';

/// One file changed by a commit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitFile {
    /// Path after the commit (the new path of a rename).
    pub path: String,
    /// Path before the commit, for a rename or copy only.
    pub old_path: Option<String>,
    /// One-letter status: `A`, `M`, `D`, `R`, `C` or `T`.
    pub status: String,
    /// Added lines; `None` for a binary file.
    pub added: Option<u64>,
    /// Removed lines; `None` for a binary file.
    pub removed: Option<u64>,
}

/// Everything the detail pane shows that the log row does not carry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitDetail {
    /// Full 40-char hash (the request may be abbreviated).
    pub hash: String,
    pub author_email: String,
    /// Full commit message (subject, blank line, body), trailing newline trimmed.
    pub message: String,
    /// Parent full hashes, first-parent first.
    pub parents: Vec<String>,
    pub files: Vec<GitCommitFile>,
    /// More than [`MAX_COMMIT_FILES`] files changed; `files` holds the first ones.
    pub files_truncated: bool,
}

/// Why a commit detail could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitDetailError {
    /// The hash is not 4–40 hex characters.
    InvalidHash,
    /// A file path is absolute, empty, or leaves the repository.
    InvalidPath,
    /// git could not be started, or it timed out.
    GitUnavailable(String),
    /// git ran and exited non-zero (unknown commit, not a repository, ...).
    GitFailed {
        command: &'static str,
        detail: String,
    },
}

impl CommitDetailError {
    /// Stable machine code shared by the Tauri `IpcResult` and the web `IpcBody`.
    pub fn code(&self) -> &'static str {
        match self {
            CommitDetailError::InvalidHash => "INVALID_COMMIT_HASH",
            CommitDetailError::InvalidPath => "INVALID_PATH",
            _ => "GIT_COMMIT_DETAIL_ERROR",
        }
    }
}

impl std::fmt::Display for CommitDetailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitDetailError::InvalidHash => write!(f, "Not a commit hash"),
            CommitDetailError::InvalidPath => write!(f, "Not a repository-relative path"),
            CommitDetailError::GitUnavailable(detail) => {
                write!(f, "Failed to run git for commit detail: {detail}")
            }
            CommitDetailError::GitFailed { command, detail } if detail.is_empty() => {
                write!(f, "git {command} failed")
            }
            CommitDetailError::GitFailed { command, detail } => {
                write!(f, "git {command} failed: {detail}")
            }
        }
    }
}

impl std::error::Error for CommitDetailError {}

/// A commit hash or an unambiguous prefix: 4 to 40 hex characters.
pub(super) fn is_commit_hash(hash: &str) -> bool {
    (4..=40).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Read the message, author email, parents and changed files of `hash` in the
/// repository at `cwd`.
pub fn git_get_commit_detail(cwd: &str, hash: &str) -> Result<GitCommitDetail, CommitDetailError> {
    if !is_commit_hash(hash) {
        log::warn!("[GitTracker] commit detail refused a non-hash in {cwd}");
        return Err(CommitDetailError::InvalidHash);
    }
    match read_commit_detail(cwd, hash) {
        Ok(detail) => {
            log::debug!(
                "[GitTracker] commit detail for {cwd} {}: {} files{}",
                detail.hash,
                detail.files.len(),
                if detail.files_truncated {
                    " (truncated)"
                } else {
                    ""
                }
            );
            Ok(detail)
        }
        Err(e) => {
            log::warn!("[GitTracker] commit detail failed for {cwd} {hash}: {e}");
            Err(e)
        }
    }
}

fn read_commit_detail(cwd: &str, hash: &str) -> Result<GitCommitDetail, CommitDetailError> {
    // %H full hash, %ae author email, %P parents, %B raw message.
    let header = run(
        cwd,
        "show",
        &[
            "show",
            "-s",
            "--no-color",
            "--format=%H%x00%ae%x00%P%x00%B",
            hash,
        ],
    )?;
    let (full_hash, author_email, parents, message) = parse_show_header(&header);
    if full_hash.is_empty() {
        return Err(CommitDetailError::GitFailed {
            command: "show",
            detail: "empty output".to_string(),
        });
    }

    let base = parents.first().map(String::as_str).unwrap_or(EMPTY_TREE);
    let numstat = run(
        cwd,
        "diff-tree",
        &[
            "diff-tree",
            "-r",
            "-z",
            "-M",
            "--no-color",
            "--numstat",
            base,
            &full_hash,
        ],
    )?;
    let name_status = run(
        cwd,
        "diff-tree",
        &[
            "diff-tree",
            "-r",
            "-z",
            "-M",
            "--no-color",
            "--name-status",
            base,
            &full_hash,
        ],
    )?;
    let (files, files_truncated) = merge_file_lists(
        parse_name_status_z(&name_status),
        parse_numstat_z(&numstat),
        MAX_COMMIT_FILES,
    );

    Ok(GitCommitDetail {
        hash: full_hash,
        author_email,
        message,
        parents,
        files,
        files_truncated,
    })
}

/// Unified diff of one file in `hash` against the commit's first parent (the
/// empty tree for a root commit), for the read-only commit Diff tab. `old_path`
/// (a rename's source) joins the pathspec so `-M` can pair the two sides.
/// Both paths must be safe repo-relative paths; the hash must be hex.
pub fn git_get_commit_file_diff(
    cwd: &str,
    hash: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<String, CommitDetailError> {
    if !is_commit_hash(hash) {
        log::warn!("[GitTracker] commit file diff refused a non-hash in {cwd}");
        return Err(CommitDetailError::InvalidHash);
    }
    let paths_ok = super::commands::is_safe_relative_path(path)
        && old_path.map_or(true, super::commands::is_safe_relative_path);
    if !paths_ok {
        log::warn!("[GitTracker] commit file diff refused an unsafe path in {cwd}");
        return Err(CommitDetailError::InvalidPath);
    }

    let parents = run(
        cwd,
        "show",
        &["show", "-s", "--no-color", "--format=%P", hash],
    )?;
    let base = parents
        .split_whitespace()
        .next()
        .unwrap_or(EMPTY_TREE)
        .to_string();
    let mut args = vec![
        "diff",
        "--no-color",
        "--no-ext-diff",
        "-M",
        &base,
        hash,
        "--",
        path,
    ];
    if let Some(old) = old_path {
        args.push(old);
    }
    let diff = run(cwd, "diff", &args)?;
    log::debug!(
        "[GitTracker] commit file diff for {cwd} {hash}: {} bytes",
        diff.len()
    );
    Ok(diff)
}

fn run(cwd: &str, command: &'static str, args: &[&str]) -> Result<String, CommitDetailError> {
    let output = GitTracker::run_git_command_with_timeout(cwd, args, COMMIT_DETAIL_GIT_TIMEOUT_MS)
        .ok_or_else(|| CommitDetailError::GitUnavailable(format!("git {command}")))?;
    if !output.status.success() {
        return Err(CommitDetailError::GitFailed {
            command,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Split the `%H%x00%ae%x00%P%x00%B` output into its four fields. The message
/// keeps its inner newlines; only the trailing ones git adds are trimmed.
pub(super) fn parse_show_header(stdout: &str) -> (String, String, Vec<String>, String) {
    let mut fields = stdout.splitn(4, FIELD_SEP);
    let hash = fields.next().unwrap_or("").trim().to_string();
    let email = fields.next().unwrap_or("").trim().to_string();
    let parents = fields
        .next()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let message = fields
        .next()
        .unwrap_or("")
        .trim_end_matches(['\n', '\r'])
        .to_string();
    (hash, email, parents, message)
}

/// `(status, old_path, path)` rows from `diff-tree -z --name-status`:
/// `M\0path\0`, or `R087\0old\0new\0` for a rename or copy.
pub(super) fn parse_name_status_z(stdout: &str) -> Vec<(String, Option<String>, String)> {
    let mut rows = Vec::new();
    let mut parts = stdout.split(FIELD_SEP).filter(|p| !p.is_empty());
    while let Some(raw_status) = parts.next() {
        let status = raw_status
            .chars()
            .next()
            .map(String::from)
            .unwrap_or_default();
        if status == "R" || status == "C" {
            let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
                break;
            };
            rows.push((status, Some(old.to_string()), new.to_string()));
        } else {
            let Some(path) = parts.next() else {
                break;
            };
            rows.push((status, None, path.to_string()));
        }
    }
    rows
}

/// `path -> (added, removed)` from `diff-tree -z --numstat`: `a\tr\tpath\0`, or
/// `a\tr\t\0old\0new\0` for a rename. `-\t-` (binary) maps to `None`.
pub(super) fn parse_numstat_z(stdout: &str) -> HashMap<String, (Option<u64>, Option<u64>)> {
    let mut stats = HashMap::new();
    let mut parts = stdout.split(FIELD_SEP);
    while let Some(head) = parts.next() {
        let head = head.trim_start_matches(['\n', '\r']);
        if head.is_empty() {
            continue;
        }
        let mut cols = head.splitn(3, '\t');
        let added = cols.next().and_then(|v| v.parse::<u64>().ok());
        let removed = cols.next().and_then(|v| v.parse::<u64>().ok());
        let inline_path = cols.next().unwrap_or("");
        let path = if inline_path.is_empty() {
            // Rename: the old and the new path follow as their own fields.
            let _old = parts.next();
            match parts.next() {
                Some(new) => new.to_string(),
                None => break,
            }
        } else {
            inline_path.to_string()
        };
        stats.insert(path, (added, removed));
    }
    stats
}

/// Join the status rows with their counts, in git's order, capped at `max`.
pub(super) fn merge_file_lists(
    rows: Vec<(String, Option<String>, String)>,
    stats: HashMap<String, (Option<u64>, Option<u64>)>,
    max: usize,
) -> (Vec<GitCommitFile>, bool) {
    let truncated = rows.len() > max;
    let files = rows
        .into_iter()
        .take(max)
        .map(|(status, old_path, path)| {
            let (added, removed) = stats.get(&path).copied().unwrap_or((None, None));
            GitCommitFile {
                path,
                old_path,
                status,
                added,
                removed,
            }
        })
        .collect();
    (files, truncated)
}
