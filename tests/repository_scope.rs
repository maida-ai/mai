use mai::repository_scope;

use repository_scope::validate_source;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Scope Test")
        .env("GIT_AUTHOR_EMAIL", "scope@example.invalid")
        .env("GIT_COMMITTER_NAME", "Scope Test")
        .env("GIT_COMMITTER_EMAIL", "scope@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--object-format=sha1"]);
    dir
}
fn commit_file(path: &Path, name: &str, contents: &str) {
    std::fs::write(path.join(name), contents).unwrap();
    git(path, &["-c", "core.hooksPath=/dev/null", "add", "--", name]);
    git(
        path,
        &["-c", "core.hooksPath=/dev/null", "commit", "-m", "fixture"],
    );
}
#[test]
fn accepts_unborn_and_committed_sha1_without_modifying_source() {
    let dir = repo();
    let validated = validate_source(dir.path()).unwrap();
    assert_eq!(validated.path, dir.path().canonicalize().unwrap());
    assert!(validated.head.is_none());
    commit_file(dir.path(), "hello", "world");
    let before = git(
        dir.path(),
        &["status", "--porcelain=v1", "--untracked-files=all"],
    );
    assert_eq!(
        validate_source(dir.path()).unwrap().head.unwrap(),
        git(dir.path(), &["rev-parse", "HEAD"])
    );
    assert_eq!(
        before,
        git(
            dir.path(),
            &["status", "--porcelain=v1", "--untracked-files=all"]
        )
    );
}
#[test]
fn rejects_sha256_and_non_repository() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_source(dir.path()).is_err());
    git(dir.path(), &["init", "--object-format=sha256"]);
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("SHA-1")
    );
}
#[test]
fn rejects_historical_lfs_pointer_even_after_deletion() {
    let dir = repo();
    commit_file(
        dir.path(),
        "large",
        "version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 1\n",
    );
    git(dir.path(), &["rm", "large"]);
    git(dir.path(), &["commit", "-m", "remove pointer"]);
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("LFS")
    );
}
#[test]
fn rejects_lfs_attributes_and_historical_gitlink() {
    let dir = repo();
    commit_file(
        dir.path(),
        ".gitattributes",
        "*.bin filter=lfs diff=lfs -text\n",
    );
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("LFS")
    );
    let dir = repo();
    commit_file(dir.path(), "hello", "world");
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    git(
        dir.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{head},module"),
        ],
    );
    git(dir.path(), &["commit", "-m", "gitlink"]);
    git(dir.path(), &["update-index", "--force-remove", "module"]);
    git(dir.path(), &["commit", "-m", "remove gitlink"]);
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("submodule")
    );
}
#[test]
fn accepts_bare_source() {
    let dir = repo();
    commit_file(dir.path(), "file", "content");
    let target = tempfile::tempdir().unwrap();
    git(
        target.path(),
        &["clone", "--bare", dir.path().to_str().unwrap(), "bare.git"],
    );
    assert!(
        validate_source(&target.path().join("bare.git"))
            .unwrap()
            .head
            .is_some()
    );
}
#[test]
fn accepts_linked_worktree() {
    let dir = repo();
    commit_file(dir.path(), "file", "content");
    let target = tempfile::tempdir().unwrap();
    let linked = target.path().join("linked");
    git(
        dir.path(),
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );
    assert!(validate_source(&linked).unwrap().head.is_some());
}
#[test]
fn rejects_lfs_on_another_branch() {
    let dir = repo();
    commit_file(dir.path(), "safe", "content");
    let safe = git(dir.path(), &["rev-parse", "HEAD"]);
    commit_file(
        dir.path(),
        "pointer",
        "version https://git-lfs.github.com/spec/v1\n",
    );
    git(dir.path(), &["branch", "unsupported"]);
    git(dir.path(), &["checkout", "--detach", &safe]);
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("LFS")
    );
}
#[test]
fn inherited_git_environment_cannot_redirect_validation() {
    let dir = repo();
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "environment_child", "--nocapture"])
        .env("MAI_SCOPE_TEST_SOURCE", dir.path())
        .env("GIT_DIR", dir.path().join("missing"))
        .env("GIT_WORK_TREE", dir.path().join("missing"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.repositoryformatversion")
        .env("GIT_CONFIG_VALUE_0", "999")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}
#[test]
fn environment_child() {
    if let Some(path) = std::env::var_os("MAI_SCOPE_TEST_SOURCE") {
        validate_source(Path::new(&path)).unwrap();
    }
}
#[cfg(unix)]
#[test]
fn preserves_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;
    let dir = repo();
    let name = std::ffi::OsString::from_vec(b"odd-\xff\nname".to_vec());
    std::fs::write(dir.path().join(name), "safe content").unwrap();
    git(dir.path(), &["add", "--all"]);
    git(dir.path(), &["commit", "-m", "non utf8"]);
    validate_source(dir.path()).unwrap();
}
#[test]
fn rejects_subdirectory_discovery_and_shallow_history() {
    let dir = repo();
    commit_file(dir.path(), "file", "content");
    let nested = dir.path().join("not-a-repo");
    std::fs::create_dir(&nested).unwrap();
    assert!(validate_source(&nested).is_err());
    std::fs::write(
        dir.path().join(".git/shallow"),
        format!("{}\n", git(dir.path(), &["rev-parse", "HEAD"])),
    )
    .unwrap();
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("shallow")
    );
}
#[test]
fn rejects_grafts_and_partial_clone_configuration() {
    let dir = repo();
    commit_file(dir.path(), "file", "content");
    std::fs::write(
        dir.path().join(".git/info/grafts"),
        "# rewritten ancestry\n",
    )
    .unwrap();
    assert!(validate_source(dir.path()).is_err());
    let dir = repo();
    git(dir.path(), &["config", "remote.origin.promisor", "true"]);
    assert!(validate_source(dir.path()).is_err());
}

#[test]
fn does_not_run_candidate_filters_or_touch_dirty_files() {
    let dir = repo();
    commit_file(dir.path(), "file", "committed");
    commit_file(dir.path(), ".gitattributes", "file filter=hostile\n");
    git(dir.path(), &["config", "filter.hostile.clean", "exit 97"]);
    git(dir.path(), &["config", "filter.hostile.smudge", "exit 97"]);
    git(dir.path(), &["config", "filter.hostile.required", "true"]);
    std::fs::write(dir.path().join("file"), "dirty content").unwrap();
    std::fs::write(dir.path().join("untracked"), "precious").unwrap();
    let index = std::fs::read(dir.path().join(".git/index")).unwrap();
    validate_source(dir.path()).unwrap();
    assert_eq!(std::fs::read(dir.path().join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read(dir.path().join("file")).unwrap(),
        b"dirty content"
    );
    assert_eq!(
        std::fs::read(dir.path().join("untracked")).unwrap(),
        b"precious"
    );
}
#[test]
fn accepts_empty_bare_repository() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--bare", "--object-format=sha1"]);
    assert!(validate_source(dir.path()).unwrap().head.is_none());
}
#[test]
fn rejects_nested_attributes_and_padded_pointer() {
    let dir = repo();
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    commit_file(
        dir.path(),
        "nested/.gitattributes",
        "[attr]large filter=lfs\n",
    );
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("LFS")
    );
    let dir = repo();
    commit_file(
        dir.path(),
        "pointer",
        &format!(
            "version https://git-lfs.github.com/spec/v1\n{}",
            "padding".repeat(4096)
        ),
    );
    assert!(
        validate_source(dir.path())
            .unwrap_err()
            .to_string()
            .contains("LFS")
    );
}

#[test]
fn rejects_missing_reachable_object() {
    let dir = repo();
    commit_file(dir.path(), "file", "unique missing object fixture");
    let oid = git(dir.path(), &["rev-parse", "HEAD:file"]);
    let loose = dir
        .path()
        .join(".git/objects")
        .join(&oid[..2])
        .join(&oid[2..]);
    std::fs::remove_file(loose).unwrap();
    assert!(validate_source(dir.path()).is_err());
}
