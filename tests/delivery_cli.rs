//! Independent Linux/macOS CLI delivery and concurrent administration qualification.
#![cfg(unix)]
use mai::{
    approval::{ApprovalStore, Decision},
    policy::Binding,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

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
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
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
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("seed");
        let root = temp.path().join("authority");
        let client = temp.path().join("client");
        let control = temp.path().join("control");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&control).unwrap();
        git(&source, &["init"]);
        fs::write(source.join("gate.sh"), "if test -f \"$MAI_CANDIDATE_ROOT/hold\"; then\n  printf ready > \"$1/ready\"\n  while ! test -f \"$1/release\"; do sleep 0.02; done\nfi\ntest ! -f \"$MAI_CANDIDATE_ROOT/fail\"\n").unwrap();
        fs::write(source.join("mai.toml"), format!("version=1\n[check]\nprogram=\"sh\"\nscript=\"gate.sh\"\nargs=[{}]\ntimeout_ms=10000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=33000\nports_per_slot=10\n[policy]\nprotected=[\"protected.txt\"]\n", serde_json::to_string(control.to_str().unwrap()).unwrap())).unwrap();
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
                .arg("clone")
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
        self.command("integrate", id)
            .args(["--mode", "textual", "--wait-seconds", "20"])
            .output()
            .unwrap()
    }
    fn show(&self, id: &str) -> Value {
        let output = self.command("show", id).output().unwrap();
        assert!(output.status.success());
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
