use mai::{
    process::{CancellationToken, ProcessOutcome, RunSpec},
    slots::{SlotPool, SlotSettings},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(PathBuf::from(mai::repository_scope::external_git_path(
            path,
        )))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn repository(root: &Path) -> PathBuf {
    let path = root.join("source");
    std::fs::create_dir(&path).unwrap();
    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "test@example.invalid"]);
    git(&path, &["config", "user.name", "Test"]);
    std::fs::write(path.join("tracked"), "clean").unwrap();
    git(&path, &["add", "."]);
    git(&path, &["commit", "-m", "initial"]);
    path
}
fn settings(caches: Vec<PathBuf>) -> SlotSettings {
    SlotSettings {
        count: 2,
        port_base: 23000,
        ports_per_slot: 10,
        caches,
    }
}
#[test]
fn independent_clones_resources_residue_and_poisoned_metadata() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let oid = git(&source, &["rev-parse", "HEAD"]);
    let pool = SlotPool::open(
        &root.path().join("pool"),
        &source,
        settings(vec!["cache".into()]),
    )
    .unwrap();
    let mut first = pool.acquire().unwrap();
    let mut second = pool.acquire().unwrap();
    assert!(pool.acquire().is_err());
    assert_ne!(first.port_base(), second.port_base());
    assert_ne!(first.temporary_path(), second.temporary_path());
    first.materialize(&oid).unwrap();
    second.materialize(&oid).unwrap();
    assert!(!first.path().join(".git/objects/info/alternates").exists());
    std::fs::create_dir(first.path().join("cache")).unwrap();
    std::fs::write(first.path().join("cache/warm"), "retain").unwrap();
    std::fs::write(first.path().join("tracked"), "dirty").unwrap();
    std::fs::write(first.path().join("residue"), "remove").unwrap();
    git(first.path(), &["config", "core.sparseCheckout", "true"]);
    std::fs::write(first.path().join(".git/info/sparse-checkout"), "none\n").unwrap();
    first.materialize(&oid).unwrap();
    assert_eq!(
        std::fs::read_to_string(first.path().join("tracked")).unwrap(),
        "clean"
    );
    assert_eq!(
        std::fs::read_to_string(first.path().join("cache/warm")).unwrap(),
        "retain"
    );
    assert!(!first.path().join("residue").exists());
    assert!(!first.path().join(".git/info/sparse-checkout").exists());
    first.release().unwrap();
    second.release().unwrap();
    let reused = pool
        .acquire()
        .expect("all successfully released slots must be reusable");
    reused.release().unwrap();
}
#[test]
fn interrupted_claim_and_failed_materialization_are_never_reused() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let pool_root = root.path().join("pool");
    let mut configuration = settings(vec![]);
    configuration.count = 1;
    let pool = SlotPool::open(&pool_root, &source, configuration.clone()).unwrap();
    let mut lease = pool.acquire().unwrap();
    assert!(lease.materialize("invalid").is_err());
    assert!(lease.release().is_err());
    let restarted = SlotPool::open(&pool_root, &source, configuration).unwrap();
    assert!(restarted.acquire().is_err());
}
#[test]
fn tracked_cache_overlap_and_invalid_settings_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let oid = git(&source, &["rev-parse", "HEAD"]);
    for (index, cache) in ["../escape", ".git/objects", "tracked"].iter().enumerate() {
        let pool = SlotPool::open(
            &root.path().join(format!("pool-{index}")),
            &source,
            settings(vec![cache.into()]),
        );
        if *cache == "tracked" {
            let mut lease = pool.unwrap().acquire().unwrap();
            assert!(lease.materialize(&oid).is_err());
            assert!(lease.release().is_err());
        } else {
            assert!(pool.is_err());
        }
    }
}
#[cfg(unix)]
#[test]
fn cache_globs_are_literal_and_symlinks_cannot_poison_reuse() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let oid = git(&source, &["rev-parse", "HEAD"]);
    let pool = SlotPool::open(
        &root.path().join("pool"),
        &source,
        settings(vec!["*".into()]),
    )
    .unwrap();
    let mut lease = pool.acquire().unwrap();
    lease.materialize(&oid).unwrap();
    std::fs::create_dir(lease.path().join("*")).unwrap();
    std::fs::write(lease.path().join("*/keep"), "cache").unwrap();
    std::fs::write(lease.path().join("residue"), "remove").unwrap();
    lease.materialize(&oid).unwrap();
    assert!(!lease.path().join("residue").exists());
    assert!(lease.path().join("*/keep").exists());
    std::fs::remove_dir_all(lease.path().join("*")).unwrap();
    std::os::unix::fs::symlink(&source, lease.path().join("*")).unwrap();
    assert!(lease.materialize(&oid).is_err());
    assert!(lease.release().is_err());
    assert!(source.join("tracked").exists());
}
#[test]
fn slot_run_injects_resources_and_private_temp() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let oid = git(&source, &["rev-parse", "HEAD"]);
    let pool = SlotPool::open(&root.path().join("pool"), &source, settings(vec![])).unwrap();
    let mut lease = pool.acquire().unwrap();
    lease.materialize(&oid).unwrap();
    #[cfg(unix)]
    let (program, args) = (
        "sh",
        vec![
            "-c".into(),
            "printf '%s:%s:' \"$MAI_SLOT\" \"$MAI_PORT_BASE\"; test -d \"$TMPDIR\"".into(),
        ],
    );
    #[cfg(windows)]
    let (program, args) = (
        "cmd.exe",
        vec![
            "/D".into(),
            "/S".into(),
            "/C".into(),
            "echo %MAI_SLOT%:%MAI_PORT_BASE%:".into(),
        ],
    );
    let result = lease
        .run(
            &RunSpec {
                program: program.into(),
                args,
                cwd: lease.path().to_owned(),
                env: Default::default(),
                timeout: Duration::from_secs(5),
                output_tail_bytes: 1024,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Success);
    assert!(result.stdout_tail.contains("0:23000:"));
    lease.release().unwrap();
}

#[cfg(unix)]
#[test]
fn replacing_clone_root_with_symlink_never_prunes_external_work() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let oid = git(&source, &["rev-parse", "HEAD"]);
    let pool = SlotPool::open(&root.path().join("pool"), &source, settings(vec![])).unwrap();
    let mut lease = pool.acquire().unwrap();
    lease.materialize(&oid).unwrap();
    std::fs::rename(lease.path(), lease.path().with_extension("moved")).unwrap();
    std::os::unix::fs::symlink(&source, lease.path()).unwrap();
    assert!(lease.release().is_err());
    assert_eq!(
        std::fs::read_to_string(source.join("tracked")).unwrap(),
        "clean"
    );
    assert!(source.join(".git").exists());
}
#[test]
fn separate_process_slot_owner_helper() {
    let Some(root) = std::env::var_os("MAI_SLOT_OWNER_DIR") else {
        return;
    };
    let root = PathBuf::from(root);
    let source = root.join("source");
    let mut settings = settings(vec![]);
    settings.count = 1;
    let pool = SlotPool::open(&root.join("pool"), &source, settings).unwrap();
    let mut lease = pool.acquire().unwrap();
    lease
        .materialize(&git(&source, &["rev-parse", "HEAD"]))
        .unwrap();
    std::fs::write(root.join("occupied-ready"), "ready").unwrap();
    std::thread::sleep(Duration::from_secs(30));
    lease.release().unwrap();
}
#[test]
fn separate_process_owner_death_preserves_claim_across_restart() {
    let root = tempfile::tempdir().unwrap();
    let source = repository(root.path());
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "separate_process_slot_owner_helper"])
        .env("MAI_SLOT_OWNER_DIR", root.path())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !root.path().join("occupied-ready").exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let mut settings = settings(vec![]);
    settings.count = 1;
    let pool = SlotPool::open(&root.path().join("pool"), &source, settings).unwrap();
    assert!(pool.acquire().is_err());
}

#[test]
fn empty_repository_marker_temp_helper() {
    let Some(root) = std::env::var_os("MAI_SLOT_MARKER_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut settings = settings(vec![]);
    settings.count = 1;
    let pool = SlotPool::open(&root.join("pool"), &root.join("source"), settings).unwrap();
    let error = match pool.acquire() {
        Ok(_) => panic!("empty .git marker must exclude temporary directory"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("temporary directory must be outside a repository"),
        "{error:#}"
    );
    assert!(root.join("pool/slot-0/occupied").exists());
}
#[test]
fn even_an_empty_git_marker_excludes_a_temporary_root() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    std::fs::create_dir(root.path().join("source")).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "empty_repository_marker_temp_helper"])
        .env("MAI_SLOT_MARKER_TEST_ROOT", root.path())
        .env("TMPDIR", root.path())
        .env("TMP", root.path())
        .env("TEMP", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
