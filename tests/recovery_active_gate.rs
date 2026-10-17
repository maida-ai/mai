//! Engine death during an active gate preserves evidence and refuses slot reuse.
//! This qualifies fail-closed restart behavior, not automatic process/slot recovery.
use mai::{
    lifecycle::{AttemptStatus, State},
    state::StateStore,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn git(root: &Path, args: &[&str]) -> String {
    success(
        Command::new("git")
            .arg("-C")
            .arg(mai::repository_scope::external_git_path(root))
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_AUTHOR_NAME", "crash-fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
            .env("GIT_COMMITTER_NAME", "crash-fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
            .output()
            .unwrap(),
    )
}
fn cli(root: &Path, command: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mai"))
        .arg("--json")
        .arg(command)
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn wait(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("fixture-owned process did not exit within the bound");
        }
        thread::sleep(Duration::from_millis(20));
    }
}
// Even an assertion failure releases this fixture's gate. Its own loop is bounded
// as a fallback; the test never kills unrelated processes or edits a slot claim.
struct GateRelease(PathBuf);
impl Drop for GateRelease {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("release"), b"release");
    }
}
fn start(root: &Path, log: &Path) -> Process {
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["serve", "--foreground"])
            .arg(root)
            .stdout(Stdio::null())
            .stderr(fs::File::create(log).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if cli(root, "status", &["--no-autostart"]).status.success() {
            return child;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "engine exited: {}",
            fs::read_to_string(log).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "engine readiness timed out: {}",
            fs::read_to_string(log).unwrap()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn engine_death_during_gate_preserves_work_and_refuses_interrupted_slot() {
    let temp = tempfile::tempdir().unwrap();
    let seed = temp.path().join("seed");
    let root = temp.path().join("authority");
    let client = temp.path().join("client");
    let control = temp.path().join("control");
    fs::create_dir(&seed).unwrap();
    fs::create_dir(&control).unwrap();
    let _release = GateRelease(control.clone());
    git(&seed, &["init"]);
    fs::write(seed.join("gate.sh"), "printf run >> \"$1/invocations\"\nprintf 'slot evidence' > \"$MAI_CANDIDATE_ROOT/crash-residue\"\nprintf ready > \"$1/ready\"\ni=0\nwhile ! test -f \"$1/release\"; do\n  i=$((i+1))\n  if test \"$i\" -ge 400; then exit 19; fi\n  sleep 0.05\ndone\nprintf done > \"$1/done\"\n").unwrap();
    fs::write(seed.join("mai.toml"), format!("version=1\n[check]\nprogram=\"sh\"\nscript=\"gate.sh\"\nargs=[{}]\ntimeout_ms=30000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=35000\nports_per_slot=10\n", serde_json::to_string(control.to_str().unwrap()).unwrap())).unwrap();
    fs::write(seed.join("work.txt"), "baseline\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "trusted crash fixture"]);
    success(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .arg("init")
            .arg(&root)
            .arg("--from")
            .arg(&seed)
            .output()
            .unwrap(),
    );
    success(
        Command::new("git")
            .args(["-c", "core.autocrlf=false", "clone"])
            .arg(root.join("authority.git"))
            .arg(&client)
            .output()
            .unwrap(),
    );
    git(&client, &["config", "core.autocrlf", "false"]);
    let mut engine = start(&root, &temp.path().join("first-engine.log"));
    success(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["receive", "enable"])
            .arg(&root)
            .args([
                "--max-input-bytes",
                "1048576",
                "--max-file-bytes",
                "65536",
                "--max-changed-files",
                "16",
            ])
            .output()
            .unwrap(),
    );
    fs::write(client.join("work.txt"), "submitted\n").unwrap();
    git(&client, &["add", "work.txt"]);
    git(&client, &["commit", "-m", "submitted change"]);
    let submitted = git(&client, &["rev-parse", "HEAD"]);
    git(
        &client,
        &["push", "-o", "agent=alice", "origin", "HEAD:refs/for/main"],
    );
    let id = mai::lifecycle::GenerationId {
        change_id: format!("alice-{submitted}"),
        generation: 1,
    };
    let authority_git = root.join("authority.git");
    let baseline = git(&authority_git, &["rev-parse", "trunk"]);
    // Work created after submission must remain byte-for-byte unchanged.
    fs::write(client.join("work.txt"), "later dirty tracked work\n").unwrap();
    fs::write(client.join("untracked.txt"), "later untracked work\n").unwrap();
    let before_status = git(&client, &["status", "--porcelain=v1"]);
    let mut request = Process(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .arg("--json")
            .arg("integrate")
            .arg(&root)
            .arg(&id.change_id)
            .args(["--mode", "textual", "--wait-seconds", "30"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while !control.join("ready").exists() {
        assert!(
            engine.0.try_wait().unwrap().is_none(),
            "engine exited before active gate"
        );
        assert!(
            request.0.try_wait().unwrap().is_none(),
            "integration exited before active gate"
        );
        assert!(Instant::now() < deadline, "gate did not become active");
        thread::sleep(Duration::from_millis(20));
    }
    let slot = root.join(".mai/validators/slot-0");
    let residue = slot.join("candidate/crash-residue");
    assert_eq!(fs::read(&residue).unwrap(), b"slot evidence");
    assert!(slot.join("occupied").is_file());
    let lifecycle = StateStore::open(root.join(".mai/state.db"))
        .unwrap()
        .load_lifecycle()
        .unwrap();
    assert!(matches!(
        lifecycle.get(&id).unwrap().state(),
        State::Validating { .. }
    ));
    engine.0.kill().unwrap();
    assert!(!engine.0.wait().unwrap().success());
    assert!(!wait(&mut request.0).success());
    assert_eq!(git(&authority_git, &["rev-parse", "trunk"]), baseline);
    let mut restarted = start(&root, &temp.path().join("restarted-engine.log"));
    let store = StateStore::open(root.join(".mai/state.db")).unwrap();
    let lifecycle = store.load_lifecycle().unwrap();
    let submission = lifecycle.get(&id).unwrap();
    assert!(matches!(submission.state(), State::Queued));
    assert_eq!(submission.attempts().len(), 1);
    assert_eq!(submission.attempts()[0].status, AttemptStatus::Interrupted);
    assert!(store.evidence_all().unwrap().is_empty());
    assert!(store.landings().unwrap().is_empty());
    let retry = cli(
        &root,
        "integrate",
        &[&id.change_id, "--mode", "textual", "--wait-seconds", "10"],
    );
    assert_eq!(retry.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&retry.stderr)
            .contains("occupied/interrupted slots require trusted cleanup"),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert!(slot.join("occupied").is_file());
    assert_eq!(fs::read(&residue).unwrap(), b"slot evidence");
    assert_eq!(
        fs::read(control.join("invocations")).unwrap(),
        b"run",
        "interrupted slot must not execute a second gate"
    );
    assert_eq!(git(&authority_git, &["rev-parse", "trunk"]), baseline);
    assert_eq!(
        git(
            &authority_git,
            &["rev-parse", &format!("refs/mai/changes/{}/1", id.change_id)]
        ),
        submitted
    );
    assert_eq!(git(&client, &["rev-parse", "HEAD"]), submitted);
    assert_eq!(git(&client, &["status", "--porcelain=v1"]), before_status);
    assert_eq!(
        fs::read(client.join("work.txt")).unwrap(),
        b"later dirty tracked work\n"
    );
    assert_eq!(
        fs::read(client.join("untracked.txt")).unwrap(),
        b"later untracked work\n"
    );
    // Release only this fixture's gate if it survived. The native watchdog/job
    // may already have terminated it; neither outcome authorizes slot reuse.
    fs::write(control.join("release"), b"release").unwrap();
    assert_eq!(git(&authority_git, &["rev-parse", "trunk"]), baseline);
    assert!(store.evidence_all().unwrap().is_empty());
    assert!(store.landings().unwrap().is_empty());
    success(cli(&root, "stop", &[]));
    assert!(wait(&mut restarted.0).success());
    assert!(slot.join("occupied").is_file());
}
