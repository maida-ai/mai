//! Independent native CLI delivery and Unix concurrent administration qualification.
#[cfg(unix)]
use mai::{
    approval::{ApprovalStore, Decision},
    policy::Binding,
};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
#[cfg(unix)]
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
};

fn bounded_utf8(bytes: &[u8], max_bytes: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut start = text.len().saturating_sub(max_bytes);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

fn succeeded(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
fn git(path: &Path, args: &[&str]) -> String {
    succeeded(
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_AUTHOR_NAME", "QA")
            .env("GIT_AUTHOR_EMAIL", "qa@localhost")
            .env("GIT_COMMITTER_NAME", "QA")
            .env("GIT_COMMITTER_EMAIL", "qa@localhost")
            .output()
            .unwrap(),
    )
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
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    client: PathBuf,
    #[cfg(unix)]
    control: PathBuf,
    owner: Process,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.owner.0.try_wait().ok().flatten().is_none() {
            let _ = self.owner.0.kill();
        }
        let _ = self.owner.0.wait();
    }
}
impl Fixture {
    fn new() -> Self {
        Self::with_gate(None)
    }
    fn with_gate(gate: Option<&str>) -> Self {
        Self::with_gate_timeout(gate, 10_000)
    }
    fn with_gate_timeout(gate: Option<&str>, gate_timeout_ms: u64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("seed");
        let root = temp.path().join("authority");
        let client = temp.path().join("client");
        let control = temp.path().join("control");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&control).unwrap();
        git(&source, &["init"]);
        fs::write(source.join("gate.sh"), gate.unwrap_or("if test -f \"$MAI_CANDIDATE_ROOT/hold\"; then\n  printf ready > \"$1/ready\"\n  while ! test -f \"$1/release\"; do sleep 0.02; done\nfi\ntest ! -f \"$MAI_CANDIDATE_ROOT/fail\"\n")).unwrap();
        fs::write(source.join("mai.toml"), format!("version=1\n[check]\nprogram=\"sh\"\nscript=\"gate.sh\"\nargs=[{}]\ntimeout_ms={gate_timeout_ms}\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=33000\nports_per_slot=10\n[policy]\nprotected=[\"protected.txt\"]\n", serde_json::to_string(control.to_str().unwrap()).unwrap())).unwrap();
        fs::write(source.join("protected.txt"), "baseline\n").unwrap();
        fs::write(source.join("overlap.txt"), "baseline\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-m", "trusted fixture policy"]);
        succeeded(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .arg("init")
                .arg(&root)
                .arg("--from")
                .arg(&source)
                .output()
                .unwrap(),
        );
        succeeded(
            Command::new("git")
                .args(["-c", "core.autocrlf=false", "clone"])
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                )
                .arg(root.join("authority.git"))
                .arg(&client)
                .output()
                .unwrap(),
        );
        let owner = Process(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["serve", "--foreground"])
                .arg(&root)
                .stdout(Stdio::null())
                .stderr(fs::File::create(temp.path().join("engine.log")).unwrap())
                .spawn()
                .unwrap(),
        );
        let mut f = Self {
            _temp: temp,
            root,
            client,
            #[cfg(unix)]
            control,
            owner,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if f.cli(&["status", "--no-autostart"]).status.success() {
                break;
            }
            assert!(
                f.owner.0.try_wait().unwrap().is_none(),
                "engine exited: {}",
                fs::read_to_string(f._temp.path().join("engine.log")).unwrap()
            );
            assert!(Instant::now() < deadline, "engine readiness timed out");
            thread::sleep(Duration::from_millis(20));
        }
        succeeded(f.cli(&[
            "receive",
            "enable",
            "--max-input-bytes",
            "1048576",
            "--max-file-bytes",
            "65536",
            "--max-changed-files",
            "32",
        ]));
        f
    }
    fn cli(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .arg("--json")
            .args(args)
            .arg(&self.root)
            .output()
            .unwrap()
    }
    fn command(&self, action: &str, id: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mai"));
        command.arg("--json").arg(action).arg(&self.root).arg(id);
        command
    }
    #[cfg(unix)]
    fn queue_command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mai"));
        command.arg("--json").arg("queue").arg(&self.root);
        command
    }
    fn acquire_lease(&self, unit: &str, owner: &str, duration_ms: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["--json", "lease", "acquire"])
            .arg(&self.root)
            .arg(unit)
            .args(["--owner", owner, "--duration-ms", duration_ms])
            .output()
            .unwrap()
    }
    fn submit(&self, files: &[(&str, &str)]) -> String {
        for (name, contents) in files {
            fs::write(self.client.join(name), contents).unwrap();
        }
        git(&self.client, &["add", "."]);
        git(&self.client, &["commit", "-m", "fixture change"]);
        let oid = git(&self.client, &["rev-parse", "HEAD"]);
        git(
            &self.client,
            &["push", "-o", "agent=qa", "origin", "HEAD:refs/for/main"],
        );
        format!("qa-{oid}")
    }
    fn integrate(&self, id: &str) -> Output {
        // Windows CI runs many Git and CLI subprocesses concurrently, and the
        // integration path includes trusted gate execution plus slot setup.
        // Keep the operation bounded while allowing that native runner enough
        // time; explicit timeout tests below retain their short budgets.
        let wait_seconds = if cfg!(windows) { 120 } else { 20 };
        let started = Instant::now();
        let mut output = self
            .command("integrate", id)
            .args(["--mode", "textual", "--wait-seconds"])
            .arg(wait_seconds.to_string())
            .output()
            .unwrap();

        // A timed-out client cannot infer the integration result. Include a
        // read-only persisted-state observation and bounded engine log tail
        // for diagnosis, without retrying or converting the timeout to success.
        if String::from_utf8_lossy(&output.stderr).contains("IPC read timed out") {
            self.append_timeout_diagnostics(id, started.elapsed(), &mut output);
        }
        output
    }
    fn append_timeout_diagnostics(&self, id: &str, elapsed: Duration, output: &mut Output) {
        let observation = match self
            .command("wait", id)
            .args(["--timeout-seconds", "1"])
            .output()
        {
            Ok(observation) => format!(
                "read-only wait exit={:?}, stdout={}, stderr={}",
                observation.status.code(),
                bounded_utf8(&observation.stdout, 1024),
                bounded_utf8(&observation.stderr, 1024)
            ),
            Err(error) => format!("read-only wait could not launch: {error}"),
        };
        let diagnostic = format!(
            "fixture diagnostic: integration elapsed={elapsed:?}; {observation}; engine log tail={}\n",
            self.bounded_engine_log_tail()
        );
        eprintln!("{diagnostic}");
        output.stderr.extend_from_slice(diagnostic.as_bytes());
    }
    fn bounded_engine_log_tail(&self) -> String {
        const LIMIT: u64 = 2048;
        let path = self._temp.path().join("engine.log");
        let read_tail = || -> std::io::Result<String> {
            let mut file = fs::File::open(&path)?;
            let length = file.metadata()?.len();
            file.seek(SeekFrom::Start(length.saturating_sub(LIMIT)))?;
            let mut bytes = Vec::with_capacity(LIMIT as usize);
            file.take(LIMIT).read_to_end(&mut bytes)?;
            Ok(bounded_utf8(&bytes, LIMIT as usize))
        };
        read_tail().unwrap_or_else(|error| format!("<engine log unavailable: {error}>"))
    }
    fn show(&self, id: &str) -> Value {
        let output = self.command("show", id).output().unwrap();
        assert!(
            output.status.success(),
            "show failed ({:?}): stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        value(&output)
    }
    fn trunk(&self) -> String {
        git(&self.root.join("authority.git"), &["rev-parse", "trunk"])
    }
    fn checkout_trunk(&self) {
        git(&self.client, &["fetch", "origin", "trunk"]);
        git(&self.client, &["checkout", "--detach", "FETCH_HEAD"]);
    }
}

#[test]
fn actual_cli_lands_dependent_submissions_once_with_exact_trees() {
    let f = Fixture::new();
    let baseline = f.trunk();
    let parent = f.submit(&[("parent.txt", "parent\n")]);
    let child = f.submit(&[("child.txt", "child\n")]);
    let blocked = f.integrate(&child);
    assert!(!blocked.status.success());
    assert_eq!(f.show(&child)["state"]["kind"], "dependency_blocked");
    assert_eq!(f.trunk(), baseline);
    let first = f.integrate(&parent);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = value(&first);
    assert_eq!(first["kind"], "landed");
    assert_eq!(first["revision"], 1);
    let second = f.integrate(&child);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second = value(&second);
    assert_eq!(second["revision"], 2);
    assert_eq!(second["kind"], "landed");
    let tip = f.trunk();
    assert_eq!(second["commit"], tip);
    for (id, result) in [(&parent, &first), (&child, &second)] {
        let state = f.show(id);
        assert_eq!(state["state"]["kind"], "terminal");
        assert_eq!(state["state"]["outcome"]["kind"], "landed");
        assert_eq!(state["state"]["outcome"]["commit"], result["commit"]);
        assert_eq!(value(&f.integrate(id)), *result);
    }
    let repo = f.root.join("authority.git");
    assert_eq!(git(&repo, &["show", "trunk:parent.txt"]), "parent");
    assert_eq!(git(&repo, &["show", "trunk:child.txt"]), "child");
    assert_eq!(
        git(&repo, &["rev-parse", "trunk^1"]),
        first["commit"].as_str().unwrap()
    );
    assert_eq!(
        git(
            &repo,
            &["rev-list", "--count", &format!("{baseline}..trunk")]
        ),
        "2"
    );
    assert_eq!(f.trunk(), tip);
    assert_eq!(
        git(&f.client, &["rev-parse", "HEAD"]),
        child.strip_prefix("qa-").unwrap(),
        "engine mutated agent branch"
    );
}

#[test]
fn actual_cli_reports_gate_and_conflict_rejection_without_moving_trunk() {
    let f = Fixture::new();
    let baseline = f.trunk();
    let failed = f.submit(&[("fail", "force trusted gate failure\n")]);
    let output = f.integrate(&failed);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(value(&output)["reason"], "check_failed");
    assert_eq!(
        f.show(&failed)["state"]["outcome"]["reason"],
        "check_failed"
    );
    assert_eq!(f.trunk(), baseline);
    f.checkout_trunk();
    let winner = f.submit(&[("overlap.txt", "winner\n")]);
    f.checkout_trunk();
    let conflict = f.submit(&[("overlap.txt", "conflict\n")]);
    let descendant = f.submit(&[("child.txt", "child of rejected change\n")]);
    assert!(f.integrate(&winner).status.success());
    let accepted = f.trunk();
    let output = f.integrate(&conflict);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(value(&output)["reason"], "conflict");
    assert_eq!(
        f.show(&descendant)["state"]["outcome"]["reason"],
        "dependency_failed"
    );
    assert_eq!(f.trunk(), accepted);
    assert_eq!(value(&f.integrate(&conflict)), value(&output));
}

#[test]
fn queue_pages_are_bounded_and_follow_stable_generation_identity_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let binary = env!("CARGO_BIN_EXE_mai");
    let queue = || {
        let mut command = Command::new(binary);
        command.args(["--json", "queue"]).arg(&root);
        command
    };
    let empty = queue().output().unwrap();
    assert!(empty.status.success());
    assert_eq!(value(&empty)["items"].as_array().unwrap().len(), 0);
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    for index in (0..105).rev() {
        let id = format!("change-{index:03}");
        let oid = format!("{:040x}", index + 1);
        life.register(&id, "fixture", &oid, vec![]).unwrap();
    }
    let large_id = mai::lifecycle::GenerationId {
        change_id: "change-050".into(),
        generation: 1,
    };
    life.reject(
        &large_id,
        mai::lifecycle::ReasonCode::CheckFailed,
        Default::default(),
    )
    .unwrap();
    for _ in 0..64 {
        life.annotate(
            &large_id,
            mai::lifecycle::Annotation {
                kind: mai::lifecycle::AnnotationKind::Recovery,
                message: "x".repeat(4096),
            },
        )
        .unwrap();
    }
    store.save_lifecycle(&mut life).unwrap();
    drop(store);
    let first = queue().args(["--limit", "100"]).output().unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = value(&first);
    assert!(serde_json::to_vec(&first).unwrap().len() < 256 * 1024);
    assert_eq!(first["items"].as_array().unwrap().len(), 100);
    assert_eq!(first["total"], 105);
    assert_eq!(first["items"][0]["id"]["change_id"], "change-000");
    assert_eq!(first["items"][99]["id"]["change_id"], "change-099");
    let summary = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"]["change_id"] == "change-050")
        .unwrap();
    assert_eq!(summary["annotation_count"], 64);
    assert!(summary.get("annotations").is_none());
    let cursor = format!(
        "{}:{}",
        first["next_cursor"]["change_id"].as_str().unwrap(),
        first["next_cursor"]["generation"].as_u64().unwrap()
    );
    let second = queue()
        .args(["--limit", "100", "--after", &cursor])
        .output()
        .unwrap();
    assert!(second.status.success());
    let second = value(&second);
    assert_eq!(second["items"].as_array().unwrap().len(), 5);
    assert_eq!(second["items"][0]["id"]["change_id"], "change-100");
    assert!(second["next_cursor"].is_null());
}

#[test]
fn wait_timeout_is_pending_and_does_not_change_received_submission() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let change_id = "waiting";
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    life.register(change_id, "fixture", &format!("{:040x}", 500), vec![])
        .unwrap();
    store.save_lifecycle(&mut life).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "wait"])
        .arg(&root)
        .arg(change_id)
        .args(["--timeout-seconds", "1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6));
    let result = value(&output);
    assert_eq!(result["kind"], "pending");
    assert_eq!(result["state"]["kind"], "received");
    let (snapshot, _, _) =
        mai::state::StateStore::observe_lifecycle(root.join(".mai/state.db")).unwrap();
    assert_eq!(
        snapshot
            .get(&mai::lifecycle::GenerationId {
                change_id: change_id.into(),
                generation: 1
            })
            .unwrap()
            .state(),
        &mai::lifecycle::State::Received
    );
}

#[cfg(unix)]
#[test]
fn wait_deadline_bounds_sqlite_lock_waits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let change_id = "locked";
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    life.register(change_id, "fixture", &format!("{:040x}", 501), vec![])
        .unwrap();
    store.save_lifecycle(&mut life).unwrap();
    drop(store);

    let lock = rusqlite::Connection::open(root.join(".mai/state.db")).unwrap();
    lock.execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE;")
        .unwrap();
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "wait"])
        .arg(&root)
        .arg(change_id)
        .args(["--timeout-seconds", "1"])
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    lock.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(output.status.code(), Some(6));
    assert_eq!(value(&output)["kind"], "pending");
    assert!(elapsed < Duration::from_secs(3), "elapsed {elapsed:?}");
}

#[test]
fn queue_rejects_authority_identity_mismatch_without_repairing_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut lifecycle = store.load_lifecycle().unwrap();
    lifecycle
        .register("identity", "fixture", &format!("{:040x}", 611), vec![])
        .unwrap();
    store.save_lifecycle(&mut lifecycle).unwrap();
    let config_path = root.join(".mai/config.toml");
    let original = fs::read_to_string(&config_path).unwrap();
    // Change only the configured identity; the read-only observer must reject the mismatch.
    let mut config: toml::Value = toml::from_str(&original).unwrap();
    config["authority_id"] = toml::Value::String("different-authority".into());
    fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let state_path = root.join(".mai/state.db");
    let before = fs::read(&state_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "queue"])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let shown = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "show"])
        .arg(&root)
        .args(["identity", "--generation", "1"])
        .output()
        .unwrap();
    assert_eq!(shown.status.code(), Some(5));
    assert_eq!(fs::read(state_path).unwrap(), before);
}

#[test]
fn wait_rejects_authority_replacement_between_polls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    let replacement = temp.path().join("replacement");
    mai::authority::Authority::init(&root, None).unwrap();
    mai::authority::Authority::init(&replacement, None).unwrap();
    for authority in [&root, &replacement] {
        let mut store = mai::state::StateStore::open(authority.join(".mai/state.db")).unwrap();
        let mut life = store.load_lifecycle().unwrap();
        life.register("same-change", "fixture", &format!("{:040x}", 502), vec![])
            .unwrap();
        if authority == &replacement {
            let id = mai::lifecycle::GenerationId {
                change_id: "same-change".into(),
                generation: 1,
            };
            life.reject(
                &id,
                mai::lifecycle::ReasonCode::CheckFailed,
                Default::default(),
            )
            .unwrap();
        }
        store.save_lifecycle(&mut life).unwrap();
    }

    let mut wait = Process(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["--json", "wait"])
            .arg(&root)
            .args(["same-change", "--timeout-seconds", "4"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    thread::sleep(Duration::from_millis(300));
    fs::copy(
        replacement.join(".mai/state.db"),
        root.join(".mai/state.db"),
    )
    .unwrap();
    fs::copy(
        replacement.join(".mai/config.toml"),
        root.join(".mai/config.toml"),
    )
    .unwrap();
    let status = wait.0.wait().unwrap();
    assert_eq!(status.code(), Some(5));
}

#[test]
fn wait_reports_landed_and_rejected_terminal_outcomes_with_show_diagnostics() {
    let f = Fixture::new();
    let landed = f.submit(&[("landed.txt", "landed\n")]);
    assert!(f.integrate(&landed).status.success());
    let result = f.command("wait", &landed).output().unwrap();
    assert!(result.status.success());
    assert_eq!(value(&result)["state"]["outcome"]["kind"], "landed");

    f.checkout_trunk();
    let rejected = f.submit(&[("fail", "force trusted gate failure\n")]);
    assert_eq!(f.integrate(&rejected).status.code(), Some(3));
    let result = f.command("wait", &rejected).output().unwrap();
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(value(&result)["state"]["outcome"]["reason"], "check_failed");
    assert!(value(&result)["state"]["outcome"]["diagnostics"]["check"].is_string());
}

#[test]
fn wait_preserves_superseded_cancelled_and_infrastructure_outcomes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    let _superseded = life
        .register("replace", "fixture", &format!("{:040x}", 710), vec![])
        .unwrap();
    life.register("replace", "fixture", &format!("{:040x}", 711), vec![])
        .unwrap();
    let cancelled = life
        .register("cancel", "fixture", &format!("{:040x}", 712), vec![])
        .unwrap();
    life.cancel(&cancelled, Default::default()).unwrap();
    let infrastructure = life
        .register("infra", "fixture", &format!("{:040x}", 713), vec![])
        .unwrap();
    life.reject(
        &infrastructure,
        mai::lifecycle::ReasonCode::Infrastructure,
        Default::default(),
    )
    .unwrap();
    store.save_lifecycle(&mut life).unwrap();

    let run = |change_id: &str| {
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["--json", "wait"])
            .arg(&root)
            .arg(change_id)
            .args(["--timeout-seconds", "1"])
            .output()
            .unwrap()
    };
    let superseded_output = run("replace");
    assert_eq!(superseded_output.status.code(), Some(3));
    assert_eq!(
        value(&superseded_output)["state"]["outcome"]["kind"],
        "superseded"
    );
    let cancelled_output = run("cancel");
    assert_eq!(cancelled_output.status.code(), Some(3));
    assert_eq!(
        value(&cancelled_output)["state"]["outcome"]["kind"],
        "cancelled"
    );
    let infrastructure_output = run("infra");
    assert_eq!(infrastructure_output.status.code(), Some(5));
    assert_eq!(
        value(&infrastructure_output)["state"]["outcome"]["reason"],
        "infrastructure"
    );
}

#[test]
fn stale_preflight_lease_claim_is_pending_without_consuming_attempt() {
    let f = Fixture::new();
    let id = f.submit(&[("work.txt", "work\n")]);
    let lease = f.acquire_lease("file:work.txt", "qa", "30000");
    assert!(lease.status.success());
    let output = f
        .command("integrate", &id)
        .args([
            "--mode",
            "textual",
            "--lease",
            "file:work.txt=99",
            "--wait-seconds",
            "15",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6));
    assert_eq!(value(&output)["kind"], "lease_blocked");
    assert_eq!(value(&output)["phase"], "preflight");
    let state = f.show(&id);
    assert_eq!(state["state"]["kind"], "queued");
    assert_eq!(state["attempts"].as_array().unwrap().len(), 0);
}

#[cfg(unix)]
#[test]
fn lease_expiry_after_passed_gate_requeues_then_explicit_fresh_retry_lands() {
    let f = Fixture::with_gate(Some("printf x >> \"$1/gate-runs\"\nsleep 2\n"));
    let baseline = f.trunk();
    let parent = f.submit(&[("work.txt", "work\n")]);
    let child = f.submit(&[("child.txt", "dependent\n")]);
    assert!(!f.integrate(&child).status.success());
    assert_eq!(f.show(&child)["state"]["kind"], "dependency_blocked");
    let first_lease = f.acquire_lease("file:work.txt", "qa", "1000");
    assert!(first_lease.status.success());
    assert_eq!(value(&first_lease)["lease"]["generation"], 1);

    let expired = f
        .command("integrate", &parent)
        .args([
            "--mode",
            "textual",
            "--lease",
            "file:work.txt=1",
            "--wait-seconds",
            "15",
        ])
        .output()
        .unwrap();
    assert_eq!(expired.status.code(), Some(6));
    let expired = value(&expired);
    assert_eq!(expired["kind"], "lease_blocked");
    assert_eq!(expired["phase"], "post_validation");
    assert_eq!(expired["reason"], "lease_expired_or_released");
    assert_eq!(f.trunk(), baseline);

    let parent_state = f.show(&parent);
    assert_eq!(parent_state["state"]["kind"], "queued");
    assert_eq!(parent_state["attempts"][0]["number"], 1);
    assert_eq!(parent_state["attempts"][0]["status"], "interrupted");
    let child_state = f.show(&child);
    assert_eq!(child_state["state"]["kind"], "dependency_blocked");
    assert_eq!(
        fs::read_to_string(f.control.join("gate-runs")).unwrap(),
        "x"
    );
    let evidence = mai::state::StateStore::open(f.root.join(".mai/state.db"))
        .unwrap()
        .evidence_all()
        .unwrap();
    assert_eq!(evidence.len(), 1);
    assert!(evidence[0].passed);
    assert_eq!(evidence[0].attempt, 1);

    let second_lease = f.acquire_lease("file:work.txt", "qa", "30000");
    assert!(second_lease.status.success());
    assert_eq!(value(&second_lease)["lease"]["generation"], 2);
    let retry = f
        .command("integrate", &parent)
        .args([
            "--mode",
            "textual",
            "--lease",
            "file:work.txt=2",
            "--wait-seconds",
            "15",
        ])
        .output()
        .unwrap();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(value(&retry)["kind"], "landed");
    assert_ne!(f.trunk(), baseline);
    assert_eq!(
        fs::read_to_string(f.control.join("gate-runs")).unwrap(),
        "xx"
    );
    let parent_state = f.show(&parent);
    assert_eq!(parent_state["state"]["outcome"]["kind"], "landed");
    assert_eq!(parent_state["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(parent_state["attempts"][1]["number"], 2);
    let evidence = mai::state::StateStore::open(f.root.join(".mai/state.db"))
        .unwrap()
        .evidence_all()
        .unwrap();
    assert_eq!(evidence.len(), 2);
    assert!(evidence.iter().all(|item| item.passed));
    assert_eq!(evidence[1].attempt, 2);
    assert_ne!(evidence[0].candidate_commit, evidence[1].candidate_commit);
    assert_ne!(f.show(&child)["state"]["kind"], "dependency_blocked");
}

#[cfg(unix)]
#[test]
fn read_only_queue_and_wait_observe_during_a_blocked_engine_gate() {
    let f = Fixture::new();
    let id = f.submit(&[("hold", "wait for explicit release\n")]);
    let mut integrate = Process(
        f.command("integrate", &id)
            .args(["--mode", "textual", "--wait-seconds", "20"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.control.join("ready").exists() {
        assert!(integrate.0.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "trusted gate did not reach marker"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let queued = f.queue_command().output().unwrap();
    assert!(
        queued.status.success(),
        "{}",
        String::from_utf8_lossy(&queued.stderr)
    );
    let shown = f.show(&id);
    assert_eq!(shown["state"]["kind"], "validating");
    assert!(shown["attempts"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
    let waiting = f
        .command("wait", &id)
        .args(["--timeout-seconds", "1"])
        .output()
        .unwrap();
    assert_eq!(waiting.status.code(), Some(6));
    assert_eq!(value(&waiting)["state"]["kind"], "validating");
    fs::write(f.control.join("release"), "release").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let result = f
            .command("wait", &id)
            .args(["--timeout-seconds", "1"])
            .output()
            .unwrap();
        if result.status.success() {
            assert_eq!(value(&result)["state"]["outcome"]["kind"], "landed");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "wait did not observe terminal landing"
        );
        thread::sleep(Duration::from_millis(30));
    }
    assert!(integrate.0.wait().unwrap().success());
}

#[cfg(unix)]
#[test]
fn wait_survives_a_gate_longer_than_ten_seconds_and_observes_landing() {
    let f = Fixture::with_gate_timeout(None, 30_000);
    let id = f.submit(&[("hold", "wait for explicit release\n")]);
    let mut integrate = Process(
        f.command("integrate", &id)
            .args(["--mode", "textual", "--wait-seconds", "30"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.control.join("ready").exists() {
        assert!(integrate.0.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "trusted gate did not reach marker"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let mut wait = Process(
        f.command("wait", &id)
            .args(["--timeout-seconds", "20"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    thread::sleep(Duration::from_millis(10_200));
    let waiting_status = wait.0.try_wait().unwrap();
    assert!(
        waiting_status.is_none(),
        "wait ended while generation remained validating: status={waiting_status:?}, current={:?}",
        f.show(&id)["state"]
    );
    fs::write(f.control.join("release"), "release").unwrap();
    assert!(wait.0.wait().unwrap().success());
    assert_eq!(f.show(&id)["state"]["outcome"]["kind"], "landed");
    assert!(integrate.0.wait().unwrap().success());
}

#[test]
fn queue_and_wait_read_persisted_state_when_engine_is_down() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let change_id = "offline";
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    life.register(change_id, "fixture", &format!("{:040x}", 600), vec![])
        .unwrap();
    store.save_lifecycle(&mut life).unwrap();
    let queued = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "queue"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        queued.status.success(),
        "{}",
        String::from_utf8_lossy(&queued.stderr)
    );
    assert_eq!(value(&queued)["items"][0]["id"]["change_id"], change_id);
    let waiting = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "wait"])
        .arg(&root)
        .arg(change_id)
        .args(["--timeout-seconds", "1"])
        .output()
        .unwrap();
    assert_eq!(waiting.status.code(), Some(6));
    assert_eq!(value(&waiting)["state"]["kind"], "received");
}

#[test]
fn wait_reports_dependency_blocked_state_and_terminal_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("authority");
    mai::authority::Authority::init(&root, None).unwrap();
    let mut store = mai::state::StateStore::open(root.join(".mai/state.db")).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    let parent = life
        .register("parent", "fixture", &format!("{:040x}", 701), vec![])
        .unwrap();
    let child = life
        .register("child", "fixture", &format!("{:040x}", 702), vec![parent])
        .unwrap();
    life.queue(&child).unwrap();
    let rejected = life
        .register("rejected", "fixture", &format!("{:040x}", 703), vec![])
        .unwrap();
    life.reject(
        &rejected,
        mai::lifecycle::ReasonCode::CheckFailed,
        Default::default(),
    )
    .unwrap();
    for _ in 0..16 {
        life.annotate(
            &rejected,
            mai::lifecycle::Annotation {
                kind: mai::lifecycle::AnnotationKind::Recovery,
                message: "x".repeat(4096),
            },
        )
        .unwrap();
    }
    store.save_lifecycle(&mut life).unwrap();
    let wait = |id: &str| {
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["--json", "wait"])
            .arg(&root)
            .arg(id)
            .args(["--timeout-seconds", "1"])
            .output()
            .unwrap()
    };
    let blocked = wait("child");
    assert_eq!(blocked.status.code(), Some(6));
    assert_eq!(value(&blocked)["state"]["kind"], "dependency_blocked");
    let rejected = wait("rejected");
    assert_eq!(rejected.status.code(), Some(3));
    assert_eq!(
        value(&rejected)["state"]["outcome"]["reason"],
        "check_failed"
    );
    assert_eq!(value(&rejected)["annotation_count"], 16);
    assert_eq!(value(&rejected)["history_omitted"], true);
    assert!(serde_json::to_vec(&value(&rejected)).unwrap().len() < 32 * 1024);
}

#[cfg(unix)]
#[test]
fn queued_denial_cannot_interleave_with_gate_and_publication() {
    let f = Fixture::new();
    let id = f.submit(&[
        ("protected.txt", "reviewed fixture change\n"),
        ("hold", "wait for test release\n"),
    ]);
    let pending = f.integrate(&id);
    assert_eq!(pending.status.code(), Some(6));
    let pending = value(&pending);
    assert_eq!(pending["kind"], "awaiting_approval");
    let candidate = pending["candidate"].as_str().unwrap();
    let approved = f
        .command("approve", candidate)
        .args([
            "--actor",
            "fixture-maintainer",
            "--reason",
            "test fixture reviewed",
        ])
        .output()
        .unwrap();
    assert!(approved.status.success());
    let binding: Binding = serde_json::from_value(value(&approved)["binding"].clone()).unwrap();
    let mut integrate = Process(
        f.command("integrate", &id)
            .args(["--mode", "textual", "--wait-seconds", "20"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.control.join("ready").exists() {
        assert!(
            integrate.0.try_wait().unwrap().is_none(),
            "integration exited before blocking gate"
        );
        assert!(
            Instant::now() < deadline,
            "trusted gate did not reach marker"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let mut socket = UnixStream::connect(f.root.join(".mai/engine.sock")).unwrap();
    writeln!(socket, "{}", json!({"protocol_version":1,"action":{"approve":{"candidate":candidate,"actor":"fixture-maintainer","reason":"queued fixture denial","denied":true}}})).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();
    let mut byte = [0];
    let error = socket.read(&mut byte).unwrap_err();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    assert_eq!(
        ApprovalStore::open(&f.root)
            .unwrap()
            .decision(&binding)
            .unwrap()
            .unwrap()
            .decision,
        Decision::Approved
    );
    assert_ne!(f.trunk(), candidate);
    fs::write(f.control.join("release"), "release").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert!(response["error"].is_null(), "{response}");
    assert_eq!(response["result"]["decision"], "denied");
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"protocol_version":1,"action":"ack"})
    )
    .unwrap();
    let mut stdout = String::new();
    integrate
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let status = integrate.0.wait().unwrap();
    assert!(status.success());
    let landed: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(landed["kind"], "landed");
    assert_eq!(f.trunk(), candidate);
    assert_eq!(
        ApprovalStore::open(&f.root)
            .unwrap()
            .decision(&binding)
            .unwrap()
            .unwrap()
            .decision,
        Decision::Approved,
        "later-base denial rewrote the exact pre-publication approval"
    );
    assert_eq!(
        response["result"]["binding"]["base"], candidate,
        "queued denial was not serialized after landing"
    );
}

#[test]
fn actual_cli_requires_live_owned_lease_generation_before_landing() {
    let mut f = Fixture::new();
    let id = f.submit(&[("leased.txt", "leased change\n")]);
    let baseline = f.trunk();
    let lease = |action: &str, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["--json", "lease", action])
            .arg(&f.root)
            .arg("file:leased.txt")
            .args(args)
            .output()
            .unwrap()
    };
    let foreign = lease("acquire", &["--owner", "other", "--duration-ms", "30000"]);
    assert!(foreign.status.success());
    assert!(
        !f.integrate(&id).status.success(),
        "foreign ownership bypassed"
    );
    assert_eq!(f.trunk(), baseline);
    assert!(
        lease("release", &["--owner", "other", "--generation", "1"])
            .status
            .success()
    );
    let acquired = lease("acquire", &["--owner", "qa", "--duration-ms", "30000"]);
    assert!(acquired.status.success());
    assert_eq!(value(&acquired)["lease"]["generation"], 2);
    assert!(
        !lease("release", &["--owner", "other", "--generation", "2"])
            .status
            .success()
    );
    assert_eq!(value(&lease("query", &[]))["lease"]["owner"], "qa");
    assert!(
        lease("release", &["--owner", "qa", "--generation", "2"])
            .status
            .success()
    );
    let acquired = lease("acquire", &["--owner", "qa", "--duration-ms", "30000"]);
    assert!(acquired.status.success());
    assert_eq!(value(&acquired)["lease"]["generation"], 3);
    assert!(
        !f.integrate(&id).status.success(),
        "active own lease accepted without claim"
    );
    let stale = f
        .command("integrate", &id)
        .args(["--mode", "textual", "--lease", "file:leased.txt=2"])
        .output()
        .unwrap();
    assert!(
        !stale.status.success(),
        "stale released generation accepted"
    );
    assert_eq!(f.trunk(), baseline);
    let accepted = f
        .command("integrate", &id)
        .args(["--mode", "textual", "--lease", "file:leased.txt=3"])
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_eq!(value(&accepted)["kind"], "landed");
    assert_eq!(f.show(&id)["state"]["outcome"]["commit"], f.trunk());
    let accepted_tip = f.trunk();
    f.owner.0.kill().unwrap();
    f.owner.0.wait().unwrap();
    let lease_path = f.root.join(".mai/leases.db");
    fs::remove_file(&lease_path).unwrap();
    assert!(
        !f.cli(&["serve", "--foreground"]).status.success(),
        "missing ownership store was silently recreated"
    );
    assert!(!lease_path.exists());
    assert_eq!(f.trunk(), accepted_tip);
}

#[cfg(unix)]
#[test]
fn timed_out_integration_keeps_running_and_exact_identity_recovers_result() {
    let mut f = Fixture::new();
    let baseline = f.trunk();
    let id = f.submit(&[("hold", "wait for explicit release\n")]);
    let integration_started = Instant::now();
    let pending = f
        .command("integrate", &id)
        .args(["--mode", "textual", "--wait-seconds", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.control.join("ready").exists() {
        assert!(Instant::now() < deadline, "gate did not start");
        thread::sleep(Duration::from_millis(10));
    }
    let mut response = pending.wait_with_output().unwrap();
    f.append_timeout_diagnostics(&id, integration_started.elapsed(), &mut response);
    assert!(!response.status.success());
    assert!(String::from_utf8_lossy(&response.stderr).contains("fixture diagnostic:"));
    assert!(
        String::from_utf8_lossy(&response.stderr).contains("outcome may be uncertain"),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&response.stdout),
        String::from_utf8_lossy(&response.stderr)
    );
    assert_eq!(f.trunk(), baseline);
    fs::write(f.control.join("release"), "continue\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let shown = loop {
        let shown = f.show(&id);
        if shown["state"]["kind"] == "terminal" {
            break shown;
        }
        assert!(
            Instant::now() < deadline,
            "generation did not finish after gate release"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(shown["state"]["kind"], "terminal");
    assert_eq!(shown["state"]["outcome"]["kind"], "landed");
    let accepted = f.trunk();
    assert_ne!(accepted, baseline);
    let retry = f.integrate(&id);
    assert!(retry.status.success());
    assert_eq!(value(&retry)["commit"], accepted);
    assert_eq!(f.trunk(), accepted);
    succeeded(f.cli(&["stop"]));
    assert!(f.owner.0.wait().unwrap().success());
    assert_eq!(f.show(&id)["state"]["outcome"]["kind"], "landed");
    f.owner = Process(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["serve", "--foreground"])
            .arg(&f.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.cli(&["status", "--no-autostart"]).status.success() {
        assert!(f.owner.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline, "restart readiness timed out");
        thread::sleep(Duration::from_millis(10));
    }
    let retried_after_restart = f.integrate(&id);
    assert!(retried_after_restart.status.success());
    assert_eq!(value(&retried_after_restart)["commit"], accepted);
    assert_eq!(value(&retried_after_restart)["revision"], 1);
    let a = mai::authority::Authority::open(&f.root).unwrap();
    let store = mai::state::StateStore::open(a.state_path()).unwrap();
    assert_eq!(store.landings().unwrap().len(), 1);
}

#[test]
fn failed_gate_exposes_bounded_diagnostics_through_show() {
    let f = Fixture::with_gate(Some(
        "i=0\nwhile test $i -lt 3000; do printf x >&2; i=$((i+1)); done\nprintf '\\nGATE_FAILURE_MARKER\\n' >&2\nexit 7\n",
    ));
    let id = f.submit(&[("new.txt", "change\n")]);
    let output = f.integrate(&id);
    assert_eq!(output.status.code(), Some(3));
    let shown = f.show(&id);
    let diagnostics = &shown["state"]["outcome"]["diagnostics"];
    assert_eq!(diagnostics["check"], "failure");
    let tail = diagnostics["log_tail"]
        .as_str()
        .expect("show must expose gate failure tail");
    assert!(tail.contains("GATE_FAILURE_MARKER"));
    assert!(tail.len() <= mai::lifecycle::MAX_DIAGNOSTIC_BYTES);
    assert!(
        diagnostics["next_action"]
            .as_str()
            .unwrap()
            .contains("new change")
    );
}
