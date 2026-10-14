use super::commit_detail::{
    is_commit_hash, merge_file_lists, parse_name_status_z, parse_numstat_z, parse_show_header,
};
use super::*;
use std::sync::atomic::AtomicU32;

static NEXT_DIR: AtomicU32 = AtomicU32::new(0);

/// Temp directory removed on drop, so tests never leak repos.
struct TestDir(std::path::PathBuf);

impl std::ops::Deref for TestDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unique_temp_dir(tag: &str) -> TestDir {
    let n = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "termul-commitdetail-{tag}-{}-{n}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    TestDir(dir)
}

fn git(cwd: &std::path::Path, args: &[&str]) -> String {
    let out =
        GitTracker::run_git_command(cwd.to_str().unwrap(), args).expect("git command should run");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo(tag: &str) -> TestDir {
    let dir = unique_temp_dir(tag);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "Test"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    git(&dir, &["config", "core.autocrlf", "false"]);
    dir
}

fn write(repo: &std::path::Path, rel: &str, contents: &[u8]) {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, contents).expect("write file");
}

fn commit(repo: &std::path::Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

fn detail(repo: &std::path::Path, hash: &str) -> GitCommitDetail {
    git_get_commit_detail(repo.to_str().unwrap(), hash).expect("commit detail")
}

fn git_missing() -> bool {
    GitTracker::run_git_command(std::env::temp_dir().to_str().unwrap(), &["--version"]).is_none()
}

// ---- pure helpers ----

#[test]
fn is_commit_hash_accepts_hex_prefixes_only() {
    assert!(is_commit_hash("abcd"));
    assert!(is_commit_hash(&"a".repeat(40)));
    assert!(is_commit_hash("E44B917"));
    assert!(!is_commit_hash("abc"));
    assert!(!is_commit_hash(&"a".repeat(41)));
    assert!(!is_commit_hash("--all"));
    assert!(!is_commit_hash("HEAD"));
    assert!(!is_commit_hash("a1b2 c3"));
}

#[test]
fn parse_show_header_keeps_the_message_body() {
    let out = "abc123\0dev@example.com\0p1 p2\0Subject line\n\nBody one\nBody two\n\n";
    let (hash, email, parents, message) = parse_show_header(out);
    assert_eq!(hash, "abc123");
    assert_eq!(email, "dev@example.com");
    assert_eq!(parents, vec!["p1".to_string(), "p2".to_string()]);
    assert_eq!(message, "Subject line\n\nBody one\nBody two");
}

#[test]
fn parse_show_header_root_commit_has_no_parents() {
    let (_, _, parents, message) = parse_show_header("abc\0e@x\0\0Init\n");
    assert!(parents.is_empty());
    assert_eq!(message, "Init");
}

#[test]
fn parse_name_status_z_reads_plain_and_rename_rows() {
    let out = "M\0src/a.rs\0R087\0old/b.rs\0new/b.rs\0D\0gone.txt\0";
    assert_eq!(
        parse_name_status_z(out),
        vec![
            ("M".to_string(), None, "src/a.rs".to_string()),
            (
                "R".to_string(),
                Some("old/b.rs".to_string()),
                "new/b.rs".to_string()
            ),
            ("D".to_string(), None, "gone.txt".to_string()),
        ]
    );
}

#[test]
fn parse_numstat_z_reads_counts_renames_and_binary() {
    let out = concat!(
        "3\t1\tsrc/a.rs\0",
        "2\t2\t\0old/b.rs\0new/b.rs\0",
        "-\t-\timg.png\0"
    );
    let stats = parse_numstat_z(out);
    assert_eq!(stats.get("src/a.rs"), Some(&(Some(3), Some(1))));
    assert_eq!(stats.get("new/b.rs"), Some(&(Some(2), Some(2))));
    assert_eq!(stats.get("img.png"), Some(&(None, None)));
    assert!(!stats.contains_key("old/b.rs"));
}

#[test]
fn merge_file_lists_keeps_order_and_caps() {
    let rows = vec![
        ("M".to_string(), None, "a".to_string()),
        ("A".to_string(), None, "b".to_string()),
        ("D".to_string(), None, "c".to_string()),
    ];
    let mut stats = HashMap::new();
    stats.insert("a".to_string(), (Some(1), Some(2)));
    stats.insert("b".to_string(), (Some(5), Some(0)));
    let (files, truncated) = merge_file_lists(rows, stats, 2);
    assert!(truncated);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "a");
    assert_eq!(files[0].added, Some(1));
    assert_eq!(files[1].status, "A");
    assert_eq!(files[1].removed, Some(0));
}

#[test]
fn invalid_hash_is_refused_before_git_runs() {
    let err = git_get_commit_detail("/definitely/not/a/repo", "--output=/tmp/x").unwrap_err();
    assert_eq!(err, CommitDetailError::InvalidHash);
    assert_eq!(err.code(), "INVALID_COMMIT_HASH");
}

// ---- real repositories ----

#[test]
fn it_root_commit_lists_added_files_against_the_empty_tree() {
    if git_missing() {
        return;
    }
    let repo = init_repo("root");
    write(&repo, "a.txt", b"one\ntwo\n");
    write(&repo, "src/b.rs", b"fn main() {}\n");
    let hash = commit(&repo, "Initial commit\n\nWith a body line.");

    let d = detail(&repo, &hash[..7]);
    assert_eq!(d.hash, hash);
    assert_eq!(d.author_email, "t@example.com");
    assert!(d.parents.is_empty());
    assert_eq!(d.message, "Initial commit\n\nWith a body line.");
    assert!(!d.files_truncated);
    let a = d.files.iter().find(|f| f.path == "a.txt").expect("a.txt");
    assert_eq!(
        (a.status.as_str(), a.added, a.removed),
        ("A", Some(2), Some(0))
    );
    assert!(d
        .files
        .iter()
        .any(|f| f.path == "src/b.rs" && f.status == "A"));
}

#[test]
fn it_detects_modify_delete_rename_and_binary() {
    if git_missing() {
        return;
    }
    let repo = init_repo("changes");
    write(&repo, "keep.txt", b"a\nb\nc\n");
    write(&repo, "drop.txt", b"x\n");
    write(
        &repo,
        "old/name.txt",
        b"line 1\nline 2\nline 3\nline 4\nline 5\n",
    );
    commit(&repo, "base");

    write(&repo, "keep.txt", b"a\nB\nc\nd\n");
    std::fs::remove_file(repo.join("drop.txt")).unwrap();
    std::fs::create_dir_all(repo.join("new")).unwrap();
    std::fs::rename(repo.join("old/name.txt"), repo.join("new/name.txt")).unwrap();
    write(&repo, "logo.bin", &[0u8, 159, 146, 150, 0, 1, 2]);
    let hash = commit(&repo, "change things");

    let d = detail(&repo, &hash);
    let by_path = |p: &str| d.files.iter().find(|f| f.path == p).cloned();
    let keep = by_path("keep.txt").expect("keep");
    assert_eq!(
        (keep.status.as_str(), keep.added, keep.removed),
        ("M", Some(2), Some(1))
    );
    assert_eq!(by_path("drop.txt").expect("drop").status, "D");
    let renamed = by_path("new/name.txt").expect("rename");
    assert_eq!(renamed.status, "R");
    assert_eq!(renamed.old_path.as_deref(), Some("old/name.txt"));
    let bin = by_path("logo.bin").expect("binary");
    assert_eq!((bin.added, bin.removed), (None, None));
}

#[test]
fn it_merge_commit_diffs_against_the_first_parent() {
    if git_missing() {
        return;
    }
    let repo = init_repo("merge");
    write(&repo, "base.txt", b"base\n");
    commit(&repo, "base");
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    write(&repo, "feature.txt", b"f1\nf2\n");
    commit(&repo, "feature work");
    git(&repo, &["checkout", "-q", "main"]);
    write(&repo, "main.txt", b"m\n");
    commit(&repo, "main work");
    git(
        &repo,
        &["merge", "-q", "--no-ff", "-m", "Merge feature", "feature"],
    );
    let merge = git(&repo, &["rev-parse", "HEAD"]);

    let d = detail(&repo, &merge);
    assert_eq!(d.parents.len(), 2);
    assert_eq!(d.message, "Merge feature");
    let paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["feature.txt"]);
}

#[test]
fn it_unknown_commit_is_a_git_failure_with_a_stable_code() {
    if git_missing() {
        return;
    }
    let repo = init_repo("unknown");
    write(&repo, "a.txt", b"a\n");
    commit(&repo, "base");
    let err = git_get_commit_detail(repo.to_str().unwrap(), &"f".repeat(40)).unwrap_err();
    assert!(matches!(
        err,
        CommitDetailError::GitFailed {
            command: "show",
            ..
        }
    ));
    assert_eq!(err.code(), "GIT_COMMIT_DETAIL_ERROR");
}

#[test]
fn commit_file_diff_refuses_bad_hash_and_unsafe_paths() {
    let cwd = "/definitely/not/a/repo";
    assert_eq!(
        git_get_commit_file_diff(cwd, "HEAD", "a.txt", None).unwrap_err(),
        CommitDetailError::InvalidHash
    );
    for bad in ["../x", "/etc/passwd", "", "a/../../b"] {
        let err = git_get_commit_file_diff(cwd, "abcd", bad, None).unwrap_err();
        assert_eq!(err, CommitDetailError::InvalidPath, "{bad}");
        assert_eq!(err.code(), "INVALID_PATH");
    }
    let err = git_get_commit_file_diff(cwd, "abcd", "ok.txt", Some("../old")).unwrap_err();
    assert_eq!(err, CommitDetailError::InvalidPath);
}

#[test]
fn it_commit_file_diff_shows_the_change_against_the_first_parent() {
    if git_missing() {
        return;
    }
    let repo = init_repo("filediff");
    write(&repo, "a.txt", b"one\ntwo\n");
    write(&repo, "other.txt", b"x\n");
    commit(&repo, "base");
    write(&repo, "a.txt", b"one\nTWO\nthree\n");
    write(&repo, "other.txt", b"y\n");
    let hash = commit(&repo, "edit");

    let diff = git_get_commit_file_diff(repo.to_str().unwrap(), &hash, "a.txt", None).unwrap();
    assert!(diff.contains("--- a/a.txt"), "{diff}");
    assert!(diff.contains("-two"));
    assert!(diff.contains("+TWO"));
    assert!(diff.contains("+three"));
    assert!(!diff.contains("other.txt"), "only the requested file");
}

#[test]
fn it_commit_file_diff_of_a_root_commit_adds_the_file() {
    if git_missing() {
        return;
    }
    let repo = init_repo("filediff-root");
    write(&repo, "new.txt", b"hello\n");
    let hash = commit(&repo, "root");
    let diff = git_get_commit_file_diff(repo.to_str().unwrap(), &hash, "new.txt", None).unwrap();
    assert!(diff.contains("new file mode"), "{diff}");
    assert!(diff.contains("+hello"));
}

#[test]
fn it_commit_file_diff_pairs_a_rename_with_its_old_path() {
    if git_missing() {
        return;
    }
    let repo = init_repo("filediff-rename");
    write(&repo, "old.txt", b"l1\nl2\nl3\nl4\nl5\nl6\n");
    commit(&repo, "base");
    std::fs::rename(repo.join("old.txt"), repo.join("new.txt")).unwrap();
    let hash = commit(&repo, "rename");
    let diff = git_get_commit_file_diff(repo.to_str().unwrap(), &hash, "new.txt", Some("old.txt"))
        .unwrap();
    assert!(diff.contains("rename from old.txt"), "{diff}");
    assert!(diff.contains("rename to new.txt"));
}
