use fs2::FileExt;
use mai::repository_scope::external_git_path;
use mai::{
    authority::Authority,
    lifecycle::{AttemptStatus, State},
    state::{StateStore, ValidationEvidence},
};
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn git(path: &Path, args: &[&str], input: Option<&[u8]>) -> String {
    let mut child = Command::new("git")
        .arg("--git-dir")
        .arg(external_git_path(path))
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn new_authority() -> (tempfile::TempDir, Authority) {
    let temp = tempfile::tempdir().unwrap();
    let authority = Authority::init(&temp.path().join("authority"), None).unwrap();
    (temp, authority)
}

fn reconcile(authority: &mut Authority) -> anyhow::Result<()> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(authority.root().join(".mai/engine.lock"))?;
    lock.try_lock_exclusive()?;
    let result = authority.reconcile();
    FileExt::unlock(&lock)?;
    result
}

fn candidate(authority: &Authority, passed: bool, manifest: Vec<String>) -> ValidationEvidence {
    let mut store = StateStore::open(authority.state_path()).unwrap();
    let parent = store.baseline().unwrap().unwrap();
    let blob = git(
        &authority.git_dir(),
        &["hash-object", "-w", "--stdin"],
        Some(b"hello\n"),
    );
    let tree = git(
        &authority.git_dir(),
        &["mktree"],
        Some(format!("100644 blob {blob}\tfile.txt\n").as_bytes()),
    );
    let submitted = git(
        &authority.git_dir(),
        &["commit-tree", &tree, "-p", &parent, "-m", "submitted"],
        None,
    );
    let mut lifecycle = store.load_lifecycle().unwrap();
    let generation = lifecycle
        .register("change", "agent", &submitted, vec![])
        .unwrap();
    lifecycle.queue(&generation).unwrap();
    let attempt = lifecycle.begin_attempt(&generation).unwrap();
    store.save_lifecycle(&mut lifecycle).unwrap();
    let policy = "1".repeat(64);
    let message = format!(
        "candidate\n\nMai-Generation: {}\nMai-Attempt: {attempt}\nMai-Revision: r1\nMai-Policy: {policy}",
        serde_json::to_string(&generation).unwrap()
    );
    let commit = git(
        &authority.git_dir(),
        &["commit-tree", &tree, "-p", &parent, "-m", &message],
        None,
    );
    let evidence = ValidationEvidence {
        generation,
        attempt,
        submitted_commit: submitted.clone(),
        candidate_commit: commit,
        candidate_tree: tree,
        candidate_parent: parent,
        trusted_policy: policy,
        passed,
        manifest,
    };
    store.record_evidence(&evidence).unwrap();
    git(
        &authority.git_dir(),
        &["update-ref", "refs/mai/changes/change/1", &submitted],
        None,
    );
    evidence
}

#[test]
fn initialization_and_repeat_preserve_exact_baseline() {
    let (_temp, authority) = new_authority();
    let before = git(
        &authority.git_dir(),
        &["rev-parse", "refs/heads/trunk"],
        None,
    );
    let reopened = Authority::init(authority.root(), None).unwrap();
    assert_eq!(
        before,
        git(&reopened.git_dir(), &["rev-parse", "HEAD"], None)
    );
    assert!(
        StateStore::open(reopened.state_path())
            .unwrap()
            .landings()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn imported_baseline_is_not_rewritten_and_source_is_untouched() {
    let (temp, original) = new_authority();
    let before = git(&original.git_dir(), &["show-ref"], None);
    let imported =
        Authority::init(&temp.path().join("imported"), Some(&original.git_dir())).unwrap();
    assert_eq!(git(&original.git_dir(), &["show-ref"], None), before);
    assert_eq!(
        git(&original.git_dir(), &["rev-parse", "HEAD"], None),
        git(&imported.git_dir(), &["rev-parse", "HEAD"], None)
    );
    assert!(!imported.git_dir().join("objects/info/alternates").exists());
}

#[test]
fn unrelated_and_interrupted_destinations_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["user-data", ".mai-initializing"] {
        let path = temp.path().join(name.replace('.', "_"));
        fs::create_dir(&path).unwrap();
        fs::write(path.join(name), b"preserve me").unwrap();
        assert!(Authority::init(&path, None).is_err());
        assert_eq!(fs::read(path.join(name)).unwrap(), b"preserve me");
    }
}

#[test]
fn corruption_and_swapped_identity_refuse_open_without_repair() {
    let (temp, authority) = new_authority();
    let other = Authority::init(&temp.path().join("other"), None).unwrap();
    fs::copy(
        other.root().join(".mai/config.toml"),
        authority.root().join(".mai/config.toml"),
    )
    .unwrap();
    assert!(Authority::open(authority.root()).is_err());
    fs::write(other.state_path(), b"not sqlite").unwrap();
    assert!(Authority::open(other.root()).is_err());
    assert_eq!(fs::read(other.state_path()).unwrap(), b"not sqlite");
}

#[test]
fn repeated_init_refuses_live_lock_but_recovers_stale_metadata() {
    let (_temp, authority) = new_authority();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(authority.root().join(".mai/engine.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(Authority::init(authority.root(), None).is_err());
    drop(lock);
    fs::write(
        authority.root().join(".mai/engine.lock"),
        b"stale pid=99999",
    )
    .unwrap();
    assert!(Authority::init(authority.root(), None).is_ok());
}

#[test]
fn receive_is_disabled_until_durable_registration_exists() {
    let (temp, authority) = new_authority();
    let remote = Authority::init(&temp.path().join("remote"), None).unwrap();
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(external_git_path(&authority.git_dir()))
        .args(["push"])
        .arg(external_git_path(&remote.git_dir()))
        .arg("HEAD:refs/heads/unsafe")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("receive is unavailable"));
}

#[test]
fn exact_passing_candidate_restores_notes_index_and_terminal_once() {
    let (_temp, mut authority) = new_authority();
    let evidence = candidate(&authority, true, vec!["file:file.txt".into()]);
    git(
        &authority.git_dir(),
        &[
            "update-ref",
            "refs/heads/trunk",
            &evidence.candidate_commit,
            &evidence.candidate_parent,
        ],
        None,
    );
    reconcile(&mut authority).unwrap();
    reconcile(&mut authority).unwrap();
    let state = StateStore::open(authority.state_path()).unwrap();
    assert_eq!(state.landings().unwrap().len(), 1);
    assert_eq!(state.unit_revisions("file:file.txt").unwrap(), vec![1]);
    assert!(matches!(
        state
            .load_lifecycle()
            .unwrap()
            .get(&evidence.generation)
            .unwrap()
            .state(),
        State::Terminal { .. }
    ));
    let notes = git(
        &authority.git_dir(),
        &[
            "notes",
            "--ref",
            "mai-objects",
            "show",
            &evidence.candidate_commit,
        ],
        None,
    );
    let note: serde_json::Value = serde_json::from_str(&notes).unwrap();
    assert_eq!(
        note["records"][0]["landed_commit"],
        evidence.candidate_commit
    );
    assert_eq!(
        note["records"][0]["entries"][0]["fallback_reason"],
        "mvp-file-only"
    );
}

#[test]
fn failed_or_wrong_manifest_evidence_never_invents_acceptance() {
    for (passed, manifest) in [
        (false, vec!["file:file.txt".into()]),
        (true, vec!["file:wrong".into()]),
    ] {
        let (_temp, mut authority) = new_authority();
        let evidence = candidate(&authority, passed, manifest);
        git(
            &authority.git_dir(),
            &["update-ref", "refs/heads/trunk", &evidence.candidate_commit],
            None,
        );
        assert!(reconcile(&mut authority).is_err());
        assert!(
            StateStore::open(authority.state_path())
                .unwrap()
                .landings()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn copied_trailers_or_missing_retention_do_not_match_identity() {
    let (_temp, mut authority) = new_authority();
    let evidence = candidate(&authority, true, vec!["file:file.txt".into()]);
    let message = git(
        &authority.git_dir(),
        &["show", "-s", "--format=%B", &evidence.candidate_commit],
        None,
    );
    let forged = git(
        &authority.git_dir(),
        &[
            "commit-tree",
            &evidence.candidate_tree,
            "-p",
            &evidence.candidate_parent,
            "-m",
            &format!("different content\n{message}"),
        ],
        None,
    );
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &forged],
        None,
    );
    assert!(reconcile(&mut authority).is_err());
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &evidence.candidate_commit],
        None,
    );
    git(
        &authority.git_dir(),
        &["update-ref", "-d", "refs/mai/changes/change/1"],
        None,
    );
    assert!(reconcile(&mut authority).is_err());
    assert!(
        StateStore::open(authority.state_path())
            .unwrap()
            .landings()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_or_conflicting_existing_notes_are_preserved() {
    for value in ["not JSON".to_owned(), json!({"format_version":1,"records":[null]}).to_string(), json!({"format_version":1,"records":[{"phase":"unknown"}]}).to_string(), json!({"format_version":1,"records":[{"phase":"landed","generation":{"change_id":"change","generation":1},"invented":true}]}).to_string()] {
        let (_temp, mut authority) = new_authority();
        let evidence = candidate(&authority, true, vec!["file:file.txt".into()]);
        git(&authority.git_dir(), &["update-ref", "refs/heads/trunk", &evidence.candidate_commit], None);
        git(&authority.git_dir(), &["notes", "--ref", "mai-objects", "add", "-m", &value, &evidence.candidate_commit], None);
        assert!(reconcile(&mut authority).is_err());
        assert_eq!(git(&authority.git_dir(), &["notes", "--ref", "mai-objects", "show", &evidence.candidate_commit], None), value);
        assert!(StateStore::open(authority.state_path()).unwrap().landings().unwrap().is_empty());
    }
}

#[test]
fn process_kill_before_and_after_cas_and_notes_converges() {
    for checkpoint in ["before-cas", "after-cas", "after-notes", "after-state"] {
        let (_temp, mut authority) = new_authority();
        let evidence = candidate(&authority, true, vec!["file:file.txt".into()]);
        let mut worker = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "crash_worker"])
            .env("MAI_CRASH_ROOT", authority.root())
            .env("MAI_CRASH_CHECKPOINT", checkpoint)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let ready = authority.root().join("checkpoint-ready");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready.exists() {
            assert!(
                worker.try_wait().unwrap().is_none(),
                "worker exited before checkpoint"
            );
            assert!(Instant::now() < deadline, "worker did not reach checkpoint");
            thread::sleep(Duration::from_millis(10));
        }
        worker.kill().unwrap();
        assert!(!worker.wait().unwrap().success());
        reconcile(&mut authority).unwrap();
        reconcile(&mut authority).unwrap();
        let store = StateStore::open(authority.state_path()).unwrap();
        let expected = usize::from(checkpoint != "before-cas");
        assert_eq!(store.landings().unwrap().len(), expected);
        if expected == 0 {
            let lifecycle = store.load_lifecycle().unwrap();
            let submission = lifecycle.get(&evidence.generation).unwrap();
            assert!(matches!(submission.state(), State::Queued));
            assert_eq!(submission.attempts()[0].status, AttemptStatus::Interrupted);
            assert_eq!(submission.attempts().len(), 1);
        }
        assert_eq!(
            git(
                &authority.git_dir(),
                &["rev-parse", "refs/heads/trunk"],
                None
            ),
            if expected == 0 {
                evidence.candidate_parent
            } else {
                evidence.candidate_commit
            }
        );
    }
}

#[test]
fn interrupted_validation_without_evidence_is_requeued() {
    let (_temp, mut authority) = new_authority();
    let mut store = StateStore::open(authority.state_path()).unwrap();
    let baseline = store.baseline().unwrap().unwrap();
    let mut lifecycle = store.load_lifecycle().unwrap();
    let id = lifecycle
        .register("no-evidence", "agent", &baseline, vec![])
        .unwrap();
    lifecycle.queue(&id).unwrap();
    lifecycle.begin_attempt(&id).unwrap();
    store.save_lifecycle(&mut lifecycle).unwrap();
    git(
        &authority.git_dir(),
        &["update-ref", "refs/mai/changes/no-evidence/1", &baseline],
        None,
    );
    reconcile(&mut authority).unwrap();
    reconcile(&mut authority).unwrap();
    let lifecycle = store.load_lifecycle().unwrap();
    assert!(matches!(lifecycle.get(&id).unwrap().state(), State::Queued));
    assert_eq!(
        lifecycle.get(&id).unwrap().attempts()[0].status,
        AttemptStatus::Interrupted
    );
}

#[test]
fn retired_attempt_evidence_cannot_accept_a_late_candidate() {
    let (_temp, mut authority) = new_authority();
    let evidence = candidate(&authority, true, vec!["file:file.txt".into()]);
    reconcile(&mut authority).unwrap();
    let mut store = StateStore::open(authority.state_path()).unwrap();
    let mut lifecycle = store.load_lifecycle().unwrap();
    assert_eq!(lifecycle.begin_attempt(&evidence.generation).unwrap(), 2);
    store.save_lifecycle(&mut lifecycle).unwrap();
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &evidence.candidate_commit],
        None,
    );
    assert!(reconcile(&mut authority).is_err());
    assert!(store.landings().unwrap().is_empty());
    assert!(
        git(
            &authority.git_dir(),
            &["notes", "--ref", "mai-objects", "list"],
            None
        )
        .is_empty()
    );
}

#[test]
fn received_record_on_same_object_survives_landed_note_rebuild() {
    let (_temp, mut authority) = new_authority();
    let evidence = candidate(&authority, true, vec!["file:file.txt".into()]);
    let received = json!({"phase":"received","generation":{"change_id":"later","generation":1},"submitted_commit":evidence.candidate_commit,"base":evidence.candidate_parent,"tree":evidence.candidate_tree,"entries":[{"unit":"file:file.txt","operation":"added","granularity":"file","parser":null,"fallback_reason":"mvp-file-only"}]});
    let note = json!({"format_version":1,"records":[received]});
    git(
        &authority.git_dir(),
        &[
            "notes",
            "--ref",
            "mai-objects",
            "add",
            "-m",
            &note.to_string(),
            &evidence.candidate_commit,
        ],
        None,
    );
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &evidence.candidate_commit],
        None,
    );
    reconcile(&mut authority).unwrap();
    reconcile(&mut authority).unwrap();
    let content = git(
        &authority.git_dir(),
        &[
            "notes",
            "--ref",
            "mai-objects",
            "show",
            &evidence.candidate_commit,
        ],
        None,
    );
    let result: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(result["records"][0], received);
    assert_eq!(result["records"].as_array().unwrap().len(), 2);
}

#[test]
#[ignore = "separate-process crash worker"]
fn crash_worker() {
    let root = std::env::var_os("MAI_CRASH_ROOT").unwrap();
    let mut authority = Authority::open(Path::new(&root)).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(authority.root().join(".mai/engine.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    let checkpoint = std::env::var("MAI_CRASH_CHECKPOINT").unwrap();
    let store = StateStore::open(authority.state_path()).unwrap();
    let evidence = store.evidence_all().unwrap().remove(0);
    if checkpoint != "before-cas" {
        git(
            &authority.git_dir(),
            &[
                "update-ref",
                "refs/heads/trunk",
                &evidence.candidate_commit,
                &evidence.candidate_parent,
            ],
            None,
        );
    }
    if checkpoint == "after-notes" {
        let record = json!({"phase":"landed","generation":evidence.generation,"submitted_commit":evidence.submitted_commit,"base":evidence.candidate_parent,"tree":evidence.candidate_tree,"landed_commit":evidence.candidate_commit,"parent":evidence.candidate_parent,"revision":1,"entries":[{"unit":"file:file.txt","operation":"added","granularity":"file","parser":null,"fallback_reason":"mvp-file-only"}]});
        let envelope = json!({"format_version":1,"records":[record]});
        git(
            &authority.git_dir(),
            &[
                "notes",
                "--ref",
                "mai-objects",
                "add",
                "-m",
                &envelope.to_string(),
                &evidence.candidate_commit,
            ],
            None,
        );
    }
    if checkpoint == "after-state" {
        authority.reconcile().unwrap();
    }
    fs::write(authority.root().join("checkpoint-ready"), checkpoint).unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(unix)]
#[test]
fn symlink_destinations_are_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    fs::create_dir(&real).unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(Authority::init(&link, None).is_err());
    assert!(fs::read_dir(real).unwrap().next().is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_authority_paths_survive_init_import_and_reconcile() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let authority = Authority::init(
        &temp
            .path()
            .join(OsString::from_vec(b"authority-\xff".to_vec())),
        None,
    )
    .unwrap();
    let imported = Authority::init(
        &temp
            .path()
            .join(OsString::from_vec(b"import-\xfe".to_vec())),
        Some(&authority.git_dir()),
    )
    .unwrap();
    let mut imported = Authority::open(imported.root()).unwrap();
    reconcile(&mut imported).unwrap();
    assert_eq!(
        git(&authority.git_dir(), &["rev-parse", "HEAD"], None),
        git(&imported.git_dir(), &["rev-parse", "HEAD"], None)
    );
}

#[test]
fn unicode_authority_paths_survive_init_import_and_reconcile() {
    let temp = tempfile::tempdir().unwrap();
    let authority = Authority::init(&temp.path().join("authority-é-日本"), None).unwrap();
    let imported = Authority::init(
        &temp.path().join("import-é-日本"),
        Some(&authority.git_dir()),
    )
    .unwrap();
    let mut imported = Authority::open(imported.root()).unwrap();
    reconcile(&mut imported).unwrap();
    assert_eq!(
        git(&authority.git_dir(), &["rev-parse", "HEAD"], None),
        git(&imported.git_dir(), &["rev-parse", "HEAD"], None)
    );
}

#[test]
fn relative_destination_aliases_initialize_and_retry_without_staging_inside() {
    for destination in [".", "./", "../work/.", "../work/../work"] {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path().join("work");
        fs::create_dir(&work).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "relative_init_worker"])
            .current_dir(&work)
            .env("MAI_INIT_PATH", destination)
            .env("MAI_INIT_CASE", "success")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "destination {destination}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!work.join(".mai-initializing").exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[test]
fn relative_nonempty_destinations_reject_without_mutation() {
    for destination in [".", "../work/."] {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path().join("work");
        fs::create_dir(&work).unwrap();
        fs::write(work.join("user-data"), b"preserve").unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "relative_init_worker"])
            .current_dir(&work)
            .env("MAI_INIT_PATH", destination)
            .env("MAI_INIT_CASE", "nonempty")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(work.join("user-data")).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(&work).unwrap().count(), 1);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[test]
#[ignore = "subprocess fixture isolating the current directory"]
fn relative_init_worker() {
    let destination = std::env::var_os("MAI_INIT_PATH").unwrap();
    let path = Path::new(&destination);
    if std::env::var("MAI_INIT_CASE").unwrap() == "nonempty" {
        assert!(Authority::init(path, None).is_err());
        return;
    }
    // A rejected import must leave the empty current directory reusable.
    assert!(Authority::init(path, Some(Path::new("missing-source"))).is_err());
    assert_eq!(fs::read_dir(".").unwrap().count(), 0);
    let authority = Authority::init(path, None).unwrap();
    let baseline = git(&authority.git_dir(), &["rev-parse", "HEAD"], None);
    let repeated = Authority::init(path, None).unwrap();
    assert_eq!(
        authority.root(),
        std::env::current_dir().unwrap().canonicalize().unwrap()
    );
    assert_eq!(
        baseline,
        git(&repeated.git_dir(), &["rev-parse", "HEAD"], None)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn unsupported_non_utf8_authority_paths_reject_without_mutation() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let sentinel = temp.path().join("user-file");
    fs::write(&sentinel, b"preserve me").unwrap();
    let unsupported = temp
        .path()
        .join(OsString::from_vec(b"authority-\xff".to_vec()));
    // Native macOS filesystems reject invalid UTF-8 names. This is an explicit
    // safe-rejection contract, not a skipped path qualification test.
    assert!(Authority::init(&unsupported, None).is_err());
    assert!(Authority::open(&unsupported).is_err());
    assert!(Authority::init(&temp.path().join("import"), Some(&unsupported)).is_err());
    assert_eq!(fs::read(&sentinel).unwrap(), b"preserve me");
    let entries: Vec<_> = fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![sentinel.file_name().unwrap().to_os_string()]);
}
