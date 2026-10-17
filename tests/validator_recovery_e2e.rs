//! Native Linux supervision qualification: kill actual fixture-owned processes,
//! then recover only when the surviving worker produced a cleanup receipt.
#![cfg(target_os = "linux")]

use mai::{
    lifecycle::{AttemptStatus, GenerationId, State},
    state::StateStore,
};
use serde_json::Value;
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
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "recovery-fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
            .env("GIT_COMMITTER_NAME", "recovery-fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
            .output()
            .unwrap(),
    )
}

fn cli(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mai"));
    command.arg("--json");
    if args[0] == "validator" {
        command.args(&args[..2]).arg(root).args(&args[2..]);
    } else {
        command.arg(args[0]).arg(root).args(&args[1..]);
    }
    command.output().unwrap()
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
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "fixture process did not exit");
        thread::sleep(Duration::from_millis(20));
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    client: PathBuf,
    control: PathBuf,
    baseline: String,
    submitted: String,
    dirty_status: String,
    id: GenerationId,
}

impl Fixture {
    fn new() -> Self {
        Self::with_bootstrap(false)
    }

    fn with_bootstrap(bootstrap: bool) -> Self {
        Self::configured(bootstrap, false)
    }

    fn configured(bootstrap: bool, large_output: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let seed = temp.path().join("seed");
        let root = temp.path().join("authority");
        let client = temp.path().join("client");
        let control = temp.path().join("control");
        fs::create_dir(&seed).unwrap();
        fs::create_dir(&control).unwrap();
        git(&seed, &["init"]);
        fs::write(seed.join("gate.sh"), "printf run >> \"$1/invocations\"\nprintf evidence > \"$MAI_CANDIDATE_ROOT/crash-residue\"\nprintf ready > \"$1/ready\"\ni=0\nwhile ! test -f \"$1/release\"; do\n  i=$((i+1))\n  if test \"$i\" -ge 600; then exit 19; fi\n  sleep 0.05\ndone\nprintf done > \"$1/done\"\n").unwrap();
        if large_output {
            fs::write(
                seed.join("gate.sh"),
                "head -c 300000 /dev/zero\nhead -c 300000 /dev/zero >&2\n",
            )
            .unwrap();
        }
        let control_arg = serde_json::to_string(control.to_str().unwrap()).unwrap();
        let check_script = if bootstrap { "check.sh" } else { "gate.sh" };
        let mut configuration = format!(
            "version=1\n[check]\nprogram=\"sh\"\nscript=\"{check_script}\"\nargs=[{control_arg}]\ntimeout_ms=60000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=38000\nports_per_slot=10\nrecovery=\"supervised-local-v1\"\n"
        );
        if bootstrap {
            fs::write(
                seed.join("check.sh"),
                "printf gate >> \"$1/check-invocations\"\n",
            )
            .unwrap();
            configuration.push_str(&format!("[validators.bootstrap]\nprogram=\"sh\"\nscript=\"gate.sh\"\nargs=[{control_arg}]\ntimeout_ms=60000\noutput_tail_bytes=1024\n"));
        }
        if large_output {
            configuration =
                configuration.replace("output_tail_bytes=1024", "output_tail_bytes=1048576");
        }
        fs::write(seed.join("mai.toml"), configuration).unwrap();
        fs::write(seed.join("work.txt"), "baseline\n").unwrap();
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "trusted recovery fixture"]);
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
        let baseline = git(&root.join("authority.git"), &["rev-parse", "trunk"]);
        // The engine is started explicitly before admission by each scenario.
        Self {
            _temp: temp,
            root,
            client,
            control,
            baseline,
            submitted: String::new(),
            dirty_status: String::new(),
            id: GenerationId {
                change_id: String::new(),
                generation: 1,
            },
        }
    }

    fn start(&self, log_name: &str) -> Process {
        let log = self._temp.path().join(log_name);
        let mut process = Process(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["serve", "--foreground"])
                .arg(&self.root)
                .stdout(Stdio::null())
                .stderr(fs::File::create(&log).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !cli(&self.root, &["status", "--no-autostart"])
            .status
            .success()
        {
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "{}",
                fs::read_to_string(&log).unwrap()
            );
            assert!(Instant::now() < deadline, "engine readiness timed out");
            thread::sleep(Duration::from_millis(20));
        }
        process
    }

    fn admit(&mut self) {
        success(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["receive", "enable"])
                .arg(&self.root)
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
        fs::write(self.client.join("work.txt"), "submitted\n").unwrap();
        git(&self.client, &["add", "work.txt"]);
        git(&self.client, &["commit", "-m", "submitted recovery change"]);
        self.submitted = git(&self.client, &["rev-parse", "HEAD"]);
        git(
            &self.client,
            &["push", "-o", "agent=alice", "origin", "HEAD:refs/for/main"],
        );
        self.id.change_id = format!("alice-{}", self.submitted);
        fs::write(self.client.join("work.txt"), "later dirty tracked work\n").unwrap();
        fs::write(self.client.join("untracked.txt"), "later untracked work\n").unwrap();
        self.dirty_status = git(&self.client, &["status", "--porcelain=v1"]);
    }

    fn integrating(&self) -> Process {
        Process(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .arg("--json")
                .arg("integrate")
                .arg(&self.root)
                .arg(&self.id.change_id)
                .args(["--mode", "textual", "--wait-seconds", "60"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    fn slot(&self) -> PathBuf {
        self.root.join(".mai/validators/slot-0")
    }

    fn await_file(&self, path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !path.is_file() {
            assert!(Instant::now() < deadline, "missing {}", path.display());
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn assert_work_preserved(&self) {
        assert_eq!(git(&self.client, &["rev-parse", "HEAD"]), self.submitted);
        assert_eq!(
            git(&self.client, &["status", "--porcelain=v1"]),
            self.dirty_status
        );
        assert_eq!(
            fs::read(self.client.join("work.txt")).unwrap(),
            b"later dirty tracked work\n"
        );
        assert_eq!(
            fs::read(self.client.join("untracked.txt")).unwrap(),
            b"later untracked work\n"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // This releases only the bounded fixture gate if a prior assertion failed.
        let _ = fs::write(self.control.join("release"), b"release");
    }
}

#[test]
fn actual_engine_death_receipt_allows_exact_recovery_and_fresh_gate_before_landing() {
    let mut fixture = Fixture::new();
    let mut engine = fixture.start("first-engine.log");
    fixture.admit();
    let mut request = fixture.integrating();
    fixture.await_file(&fixture.control.join("ready"));
    engine.0.kill().unwrap();
    assert!(!engine.0.wait().unwrap().success());
    assert!(!wait(&mut request.0).success());
    fixture.await_file(&fixture.slot().join("cleanup-receipt.json"));
    assert!(!fixture.control.join("done").exists());
    assert_eq!(
        git(&fixture.root.join("authority.git"), &["rev-parse", "trunk"]),
        fixture.baseline
    );
    let inspection: Value = serde_json::from_str(&success(cli(
        &fixture.root,
        &["validator", "inspect", "--slot", "0"],
    )))
    .unwrap();
    let claim = inspection["claim_id"].as_str().unwrap();
    assert_eq!(inspection["cleanup_receipt"], true);
    let recovered: Value = serde_json::from_str(&success(cli(
        &fixture.root,
        &["validator", "recover", "--slot", "0", "--claim", claim],
    )))
    .unwrap();
    let repeated: Value = serde_json::from_str(&success(cli(
        &fixture.root,
        &["validator", "recover", "--slot", "0", "--claim", claim],
    )))
    .unwrap();
    assert_eq!(
        recovered, repeated,
        "exact recovery retry must be idempotent"
    );
    assert!(!fixture.slot().join("occupied").exists());
    let store = StateStore::open(fixture.root.join(".mai/state.db")).unwrap();
    let lifecycle = store.load_lifecycle().unwrap();
    let submission = lifecycle.get(&fixture.id).unwrap();
    assert!(matches!(submission.state(), State::Queued));
    assert_eq!(submission.attempts()[0].status, AttemptStatus::Interrupted);
    assert!(store.evidence_all().unwrap().is_empty());
    assert!(store.landings().unwrap().is_empty());
    fs::write(fixture.control.join("release"), b"release").unwrap();
    let mut restarted = fixture.start("restarted-engine.log");
    success(cli(
        &fixture.root,
        &[
            "integrate",
            &fixture.id.change_id,
            "--mode",
            "textual",
            "--wait-seconds",
            "30",
        ],
    ));
    assert_eq!(
        fs::read(fixture.control.join("invocations")).unwrap(),
        b"runrun"
    );
    assert!(fixture.control.join("done").exists());
    let store = StateStore::open(fixture.root.join(".mai/state.db")).unwrap();
    assert_eq!(store.landings().unwrap().len(), 1);
    assert_eq!(store.evidence_all().unwrap().len(), 1);
    let lifecycle = store.load_lifecycle().unwrap();
    let submission = lifecycle.get(&fixture.id).unwrap();
    assert_eq!(submission.attempts().len(), 2);
    assert_eq!(submission.attempts()[0].status, AttemptStatus::Interrupted);
    fixture.assert_work_preserved();
    success(cli(&fixture.root, &["stop"]));
    assert!(wait(&mut restarted.0).success());
}

#[test]
fn actual_worker_death_never_creates_recovery_proof() {
    let mut fixture = Fixture::new();
    let mut engine = fixture.start("worker-death-engine.log");
    fixture.admit();
    let mut request = fixture.integrating();
    fixture.await_file(&fixture.control.join("ready"));
    let children = fs::read_to_string(format!(
        "/proc/{}/task/{}/children",
        engine.0.id(),
        engine.0.id()
    ))
    .unwrap();
    let workers: Vec<u32> = children
        .split_whitespace()
        .filter_map(|pid| {
            let command = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            let args: Vec<_> = command.split(|byte| *byte == 0).collect();
            args.windows(2)
                .any(|pair| pair == [b"validator".as_slice(), b"worker".as_slice()])
                .then(|| pid.parse().unwrap())
        })
        .collect();
    assert_eq!(
        workers.len(),
        1,
        "identify exactly this fixture's native worker"
    );
    // SAFETY: PID was selected only from this owned engine's direct children,
    // with its exact hidden-worker argv, while that worker's gate is active.
    assert_eq!(
        unsafe { libc::kill(workers[0] as libc::pid_t, libc::SIGKILL) },
        0
    );
    assert!(!wait(&mut request.0).success());
    engine.0.kill().unwrap();
    assert!(!engine.0.wait().unwrap().success());
    assert!(!fixture.slot().join("cleanup-receipt.json").exists());
    let occupied = fs::read(fixture.slot().join("occupied")).unwrap();
    let claim: Value = serde_json::from_slice(&occupied).unwrap();
    let claim_id = claim["claim_id"].as_str().unwrap();
    let recovery = cli(
        &fixture.root,
        &["validator", "recover", "--slot", "0", "--claim", claim_id],
    );
    assert!(!recovery.status.success());
    assert_eq!(fs::read(fixture.slot().join("occupied")).unwrap(), occupied);
    assert!(fixture.slot().join("candidate/crash-residue").exists());
    assert_eq!(
        git(&fixture.root.join("authority.git"), &["rev-parse", "trunk"]),
        fixture.baseline
    );
    fixture.assert_work_preserved();
}

#[test]
fn actual_engine_death_during_bootstrap_recovers_without_claiming_a_gate_pass() {
    let mut fixture = Fixture::with_bootstrap(true);
    let mut engine = fixture.start("bootstrap-engine.log");
    fixture.admit();
    let mut request = fixture.integrating();
    fixture.await_file(&fixture.control.join("ready"));
    assert!(!fixture.control.join("check-invocations").exists());
    engine.0.kill().unwrap();
    assert!(!engine.0.wait().unwrap().success());
    assert!(!wait(&mut request.0).success());
    fixture.await_file(&fixture.slot().join("cleanup-receipt.json"));
    let inspection: Value = serde_json::from_str(&success(cli(
        &fixture.root,
        &["validator", "inspect", "--slot", "0"],
    )))
    .unwrap();
    let claim = inspection["claim_id"].as_str().unwrap();
    success(cli(
        &fixture.root,
        &["validator", "recover", "--slot", "0", "--claim", claim],
    ));
    let store = StateStore::open(fixture.root.join(".mai/state.db")).unwrap();
    assert!(store.evidence_all().unwrap().is_empty());
    assert!(store.landings().unwrap().is_empty());
    assert!(!fixture.control.join("check-invocations").exists());
    fs::write(fixture.control.join("release"), b"release").unwrap();
    let mut restarted = fixture.start("bootstrap-restarted-engine.log");
    success(cli(
        &fixture.root,
        &[
            "integrate",
            &fixture.id.change_id,
            "--mode",
            "textual",
            "--wait-seconds",
            "30",
        ],
    ));
    assert_eq!(
        fs::read(fixture.control.join("invocations")).unwrap(),
        b"runrun"
    );
    assert_eq!(
        fs::read(fixture.control.join("check-invocations")).unwrap(),
        b"gate"
    );
    let store = StateStore::open(fixture.root.join(".mai/state.db")).unwrap();
    assert_eq!(store.landings().unwrap().len(), 1);
    assert_eq!(store.evidence_all().unwrap().len(), 1);
    fixture.assert_work_preserved();
    success(cli(&fixture.root, &["stop"]));
    assert!(wait(&mut restarted.0).success());
}

#[test]
fn supervised_worker_preserves_valid_tails_that_expand_beyond_one_megabyte_in_json() {
    let fixture = Fixture::configured(false, true);
    let output = cli(&fixture.root, &["validator", "run", &fixture.baseline]);
    assert!(
        output.status.success(),
        "validator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "success");
    for name in ["stdout_tail", "stderr_tail"] {
        let tail = result["process"][name].as_str().unwrap();
        assert_eq!(tail.len(), 300000);
        assert!(tail.bytes().all(|byte| byte == 0));
    }
    assert!(!fixture.slot().join("occupied").exists());
}
