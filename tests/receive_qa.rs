//! Independent end-to-end admission checks through real receive-pack and engine processes.
use mai::{authority::Authority, state::StateStore};
use std::{
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
fn git(path: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("LC_ALL", "C")
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
        .unwrap()
}
fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(args)
        .arg(root)
        .output()
        .unwrap()
}
struct Owner(Child);
impl Drop for Owner {
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
}
impl Fixture {
    fn new() -> Self {
        Self::with_gate("exit 0\n")
    }
    fn with_gate(gate: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("authority");
        let client = temp.path().join("client");
        let seed = temp.path().join("seed");
        std::fs::create_dir(&seed).unwrap();
        success(
            Command::new("git")
                .args(["init", "-b", "main"])
                .arg(&seed)
                .output()
                .unwrap(),
        );
        std::fs::write(seed.join("gate.sh"), gate).unwrap();
        std::fs::write(
            seed.join("mai.toml"),
            "version=1\n[check]\nprogram=\"sh\"\nscript=\"gate.sh\"\ntimeout_ms=20000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=41000\nports_per_slot=10\n",
        )
        .unwrap();
        success(git(&seed, &["add", "."]));
        success(git(&seed, &["commit", "-m", "trusted policy"]));
        // Initialize from a trusted seed so the automatic integration gate can pass.
        success(cli(&root, &["init", "--from", seed.to_str().unwrap()]));
        success(
            Command::new("git")
                .args(["clone", "--no-local"])
                .arg(root.join("authority.git"))
                .arg(&client)
                .output()
                .unwrap(),
        );
        Self {
            _temp: temp,
            root,
            client,
        }
    }
    fn clone_client(&self, name: &str) -> PathBuf {
        let client = self._temp.path().join(name);
        success(
            Command::new("git")
                .args(["clone", "--no-local"])
                .arg(self.root.join("authority.git"))
                .arg(&client)
                .output()
                .unwrap(),
        );
        client
    }
    fn start(&self) -> Owner {
        let mut owner = Owner(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["serve", "--foreground"])
                .arg(&self.root)
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(self.root.join("engine-qa.log")).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if cli(&self.root, &["status", "--no-autostart"])
                .status
                .success()
            {
                return owner;
            }
            assert!(
                owner.0.try_wait().unwrap().is_none(),
                "engine exited before readiness: {}",
                std::fs::read_to_string(self.root.join("engine-qa.log")).unwrap_or_default()
            );
            assert!(Instant::now() < deadline, "engine readiness timed out");
            thread::sleep(Duration::from_millis(25));
        }
    }
    fn enable_mode(&self, auto_integrate: bool) {
        let mut args = vec![
            "receive",
            "enable",
            "--max-input-bytes",
            "1048576",
            "--max-file-bytes",
            "65536",
            "--max-changed-files",
            "16",
        ];
        if auto_integrate {
            args.push("--auto-integrate");
        }
        success(cli(&self.root, &args));
    }
    fn enable(&self) {
        self.enable_mode(false);
    }
    fn commit(&self, name: &str) -> String {
        std::fs::write(self.client.join(name), name).unwrap();
        success(git(&self.client, &["add", name]));
        success(git(&self.client, &["commit", "-m", name]));
        success(git(&self.client, &["rev-parse", "HEAD"]))
    }
    fn push(&self, refs: &[&str]) -> Output {
        let mut args = vec!["push", "-o", "agent=qa", "origin"];
        args.extend_from_slice(refs);
        git(&self.client, &args)
    }
    fn submissions(&self) -> Vec<mai::lifecycle::Submission> {
        let authority = Authority::open(&self.root).unwrap();
        StateStore::open(authority.state_path())
            .unwrap()
            .load_lifecycle()
            .unwrap()
            .submissions()
            .cloned()
            .collect()
    }
}

fn receipt(output: Output) -> String {
    let diagnostics = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "{diagnostics}");
    assert!(
        diagnostics.contains("pending; durable receipt only"),
        "{diagnostics}"
    );
    assert!(!diagnostics.contains("not landed"), "{diagnostics}");
    diagnostics
}

fn wait_terminal(f: &Fixture, count: usize, timeout: Duration) -> Vec<mai::lifecycle::Submission> {
    let deadline = Instant::now() + timeout;
    loop {
        let rows = f.submissions();
        if rows.len() == count && rows.iter().all(|s| s.state().is_terminal()) {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "terminal outcomes timed out: {rows:?}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

// Each gate exceeds the former receipt deadline. Two immediate submissions per
// agent also exercise the exact submitted-parent mapping while its gate runs.
#[test]
fn slow_gates_keep_three_agents_receiving_successive_work() {
    concurrent_automatic_receipts(11, 2);
}

// Each gate fits within ten seconds, but the serial queue does not.
#[test]
fn cumulative_gates_do_not_delay_concurrent_receipts() {
    concurrent_automatic_receipts(4, 1);
}

fn concurrent_automatic_receipts(seconds: u64, rounds: usize) {
    let f = Fixture::with_gate(&format!(
        "sleep {seconds}\nfor file in \"$MAI_CANDIDATE_ROOT\"/*.txt; do test -f \"$file\" || continue; test \"$(cat \"$file\")\" = \"$(basename \"$file\")\" || exit 1; done\n"
    ));
    let _owner = f.start();
    f.enable_mode(true);
    let clients = [
        f.client.clone(),
        f.clone_client("second"),
        f.clone_client("third"),
    ];
    let started = Instant::now();
    let pushes: Vec<_> = clients
        .iter()
        .cloned()
        .enumerate()
        .map(|(agent, client)| {
            thread::spawn(move || {
                let mut commits = Vec::new();
                for round in 0..rounds {
                    let name = format!("agent-{agent}-{round}.txt");
                    std::fs::write(client.join(&name), &name).unwrap();
                    success(git(&client, &["add", &name]));
                    success(git(&client, &["commit", "-m", &name]));
                    let commit = success(git(&client, &["rev-parse", "HEAD"]));
                    let receipt_started = Instant::now();
                    receipt(git(
                        &client,
                        &[
                            "push",
                            "-o",
                            &format!("agent=agent-{agent}"),
                            "origin",
                            "HEAD:refs/for/main",
                        ],
                    ));
                    assert!(
                        receipt_started.elapsed() < Duration::from_secs(10),
                        "receipt waited for a gate"
                    );
                    commits.push(commit);
                }
                commits
            })
        })
        .collect();
    let commits: Vec<_> = pushes.into_iter().flat_map(|p| p.join().unwrap()).collect();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "concurrent receipts waited for serial gates"
    );
    let received = f.submissions();
    assert_eq!(
        received.len(),
        3 * rounds,
        "receipt must already be durable"
    );
    let waiting = received
        .iter()
        .find(|s| matches!(s.state(), mai::lifecycle::State::Received))
        .or_else(|| received.iter().find(|s| !s.state().is_terminal()))
        .unwrap();
    let wait_started = Instant::now();
    let pending = mai::engine::wait(&f.root, waiting.id().clone(), Duration::from_secs(1)).unwrap();
    assert_eq!(pending["kind"], "pending");
    assert!(wait_started.elapsed() < Duration::from_secs(3));
    let before_retry = f.submissions();
    let pending_row = before_retry
        .iter()
        .find(|row| row.id() == waiting.id())
        .unwrap();
    assert!(matches!(
        pending_row.state(),
        mai::lifecycle::State::Received
    ));
    let client_index = commits
        .iter()
        .position(|candidate| candidate == pending_row.commit())
        .unwrap();
    let client = &clients[client_index / rounds];
    let retry_started = Instant::now();
    let retry = receipt(git(
        client,
        &[
            "push",
            "-o",
            &format!("agent=agent-{}", client_index / rounds),
            "origin",
            &format!("{}:refs/for/main", pending_row.commit()),
        ],
    ));
    assert!(
        retry_started.elapsed() < Duration::from_secs(10),
        "pending retry waited for a gate"
    );
    assert!(retry.contains("pending; durable receipt only"), "{retry}");
    assert_eq!(
        f.submissions(),
        before_retry,
        "identical pending retry changed durable submissions"
    );
    let stop = cli(&f.root, &["stop"]);
    let stop_diagnostics = String::from_utf8_lossy(&stop.stderr);
    assert!(
        !stop.status.success(),
        "busy stop unexpectedly stopped the engine"
    );
    assert!(
        stop_diagnostics
            .contains("integration is active; retry stop after its outcome is recorded"),
        "{stop_diagnostics}"
    );
    assert!(
        !stop_diagnostics.contains("engine unavailable"),
        "{stop_diagnostics}"
    );
    assert!(cli(&f.root, &["status", "--no-autostart"]).status.success());
    let rows = wait_terminal(
        &f,
        3 * rounds,
        Duration::from_secs(seconds * 3 * rounds as u64 + 30),
    );
    let repo = f.root.join("authority.git");
    let store = StateStore::open(Authority::open(&f.root).unwrap().state_path()).unwrap();
    let evidence = store.evidence_all().unwrap();
    assert_eq!(evidence.len(), rows.len());
    for row in &rows {
        assert!(commits.iter().any(|oid| oid == row.commit()));
        let mai::lifecycle::State::Terminal {
            outcome: mai::lifecycle::Outcome::Landed { commit, .. },
        } = row.state()
        else {
            panic!("submission did not land: {row:?}");
        };
        let proof = evidence.iter().find(|e| &e.generation == row.id()).unwrap();
        assert!(proof.passed);
        assert_eq!(&proof.submitted_commit, row.commit());
        assert_eq!(
            success(git(&repo, &["rev-parse", &format!("{commit}^{{tree}}")])),
            proof.candidate_tree
        );
        assert_eq!(
            success(git(
                &repo,
                &["rev-parse", &mai::receive::retained_ref(row.id())]
            )),
            row.commit()
        );
    }
    for agent in 0..3 {
        for round in 0..rounds {
            let name = format!("agent-{agent}-{round}.txt");
            assert_eq!(
                success(git(&repo, &["show", &format!("refs/heads/trunk:{name}")])),
                name
            );
        }
    }
    // A retry observes the already persisted terminal outcome, not a generic receipt.
    for (agent, client) in clients.iter().enumerate() {
        let output = git(
            client,
            &[
                "push",
                "-o",
                &format!("agent=agent-{agent}"),
                "origin",
                "HEAD:refs/for/main",
            ],
        );
        let diagnostics = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(output.status.success(), "{diagnostics}");
        assert!(diagnostics.contains("landed"), "{diagnostics}");
        assert!(
            !diagnostics.contains("pending") && !diagnostics.contains("not landed"),
            "{diagnostics}"
        );
    }
    assert_eq!(f.submissions(), rows);
}

#[test]
fn automatic_integration_lands_with_trusted_policy() {
    let f = Fixture::new();
    let _owner = f.start();
    f.enable_mode(true);
    f.commit("automatic.txt");
    success(f.push(&["HEAD:refs/for/main"]));
    let deadline = Instant::now() + Duration::from_secs(15);
    let submissions = loop {
        let rows = f.submissions();
        if rows.first().is_some_and(|s| {
            matches!(
                s.state(),
                mai::lifecycle::State::Terminal {
                    outcome: mai::lifecycle::Outcome::Landed { .. }
                }
            )
        }) {
            break rows;
        }
        assert!(
            Instant::now() < deadline,
            "automatic integration did not land: {rows:?}"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(submissions.len(), 1);
    let landed = match submissions[0].state() {
        mai::lifecycle::State::Terminal {
            outcome: mai::lifecycle::Outcome::Landed { commit, .. },
        } => commit,
        state => panic!("expected landed terminal state, got {state:?}"),
    };
    let trunk = success(git(
        &f.root.join("authority.git"),
        &["rev-parse", "refs/heads/trunk"],
    ));
    assert_eq!(trunk, *landed);
    assert_eq!(
        success(git(
            &f.root.join("authority.git"),
            &["show", "refs/heads/trunk:automatic.txt"],
        )),
        "automatic.txt"
    );
}

#[test]
fn automatic_gate_rejection_does_not_block_independent_landing() {
    let f = Fixture::with_gate("test ! -f \"$MAI_CANDIDATE_ROOT/reject\"\n");
    let _owner = f.start();
    f.enable_mode(true);
    let good_client = f.clone_client("good-client");

    let rejected_commit = f.commit("reject");
    success(f.push(&["HEAD:refs/for/main"]));
    let good_commit = {
        std::fs::write(good_client.join("independent.txt"), "independent").unwrap();
        success(git(&good_client, &["add", "independent.txt"]));
        success(git(&good_client, &["commit", "-m", "independent"]));
        success(git(&good_client, &["rev-parse", "HEAD"]))
    };
    success(git(
        &good_client,
        &["push", "-o", "agent=good", "origin", "HEAD:refs/for/main"],
    ));

    let deadline = Instant::now() + Duration::from_secs(15);
    let submissions = loop {
        let rows = f.submissions();
        let terminal = |commit: &str| {
            rows.iter()
                .find(|s| s.commit() == commit)
                .is_some_and(|s| matches!(s.state(), mai::lifecycle::State::Terminal { .. }))
        };
        if terminal(&rejected_commit) && terminal(&good_commit) {
            break rows;
        }
        assert!(
            Instant::now() < deadline,
            "automatic outcomes missing: {rows:?}"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(submissions.len(), 2);
    let rejected = submissions
        .iter()
        .find(|s| s.commit() == rejected_commit)
        .unwrap();
    assert!(matches!(
        rejected.state(),
        mai::lifecycle::State::Terminal {
            outcome: mai::lifecycle::Outcome::Rejected {
                reason: mai::lifecycle::ReasonCode::CheckFailed,
                ..
            }
        }
    ));
    let good = submissions
        .iter()
        .find(|s| s.commit() == good_commit)
        .unwrap();
    let landed = match good.state() {
        mai::lifecycle::State::Terminal {
            outcome: mai::lifecycle::Outcome::Landed { commit, .. },
        } => commit,
        state => panic!("expected good submission landed, got {state:?}"),
    };
    let retry = f.push(&["HEAD:refs/for/main"]);
    let diagnostics = String::from_utf8_lossy(&retry.stderr).into_owned();
    assert!(retry.status.success(), "{diagnostics}");
    assert!(
        diagnostics.contains("rejected") && diagnostics.contains("check_failed"),
        "{diagnostics}"
    );
    assert!(
        !diagnostics.contains("pending") && !diagnostics.contains("not landed"),
        "{diagnostics}"
    );
    let repo = f.root.join("authority.git");
    assert_eq!(
        success(git(&repo, &["rev-parse", "refs/heads/trunk"])),
        *landed
    );
    assert_eq!(
        success(git(&repo, &["show", "refs/heads/trunk:independent.txt"])),
        "independent"
    );
    assert!(
        !git(&repo, &["cat-file", "-e", "refs/heads/trunk:reject"])
            .status
            .success()
    );
    for submission in [rejected, good] {
        let retained = mai::receive::retained_ref(submission.id());
        assert_eq!(
            success(git(&repo, &["rev-parse", &retained])),
            submission.commit()
        );
    }
}

#[test]
fn concurrent_submissions_are_serially_registered_without_auto_mode() {
    let f = Fixture::new();
    let _owner = f.start();
    f.enable();
    let second = f._temp.path().join("second-client");
    success(
        Command::new("git")
            .args(["clone", "--no-local"])
            .arg(f.root.join("authority.git"))
            .arg(&second)
            .output()
            .unwrap(),
    );
    let first_commit = f.commit("first.txt");
    let second_commit = {
        std::fs::write(second.join("second.txt"), "second").unwrap();
        success(git(&second, &["add", "second.txt"]));
        success(git(&second, &["commit", "-m", "second"]));
        success(git(&second, &["rev-parse", "HEAD"]))
    };
    let root = f.root.clone();
    let first_client = f.client.clone();
    let second_client = second.clone();
    let pushes = [
        thread::spawn(move || {
            git(
                &first_client,
                &["push", "-o", "agent=first", "origin", "HEAD:refs/for/main"],
            )
        }),
        thread::spawn(move || {
            git(
                &second_client,
                &["push", "-o", "agent=second", "origin", "HEAD:refs/for/main"],
            )
        }),
    ];
    for push in pushes {
        success(push.join().unwrap());
    }
    let rows = StateStore::open(Authority::open(&root).unwrap().state_path())
        .unwrap()
        .load_lifecycle()
        .unwrap();
    let submissions: Vec<_> = rows.submissions().collect();
    assert_eq!(submissions.len(), 2);
    assert!(submissions.iter().any(|s| s.commit() == first_commit));
    assert!(submissions.iter().any(|s| s.commit() == second_commit));
    assert!(
        submissions
            .iter()
            .all(|s| matches!(s.state(), mai::lifecycle::State::Received))
    );
}

#[test]
fn acknowledged_submission_is_retained_and_survives_owner_death_once() {
    let f = Fixture::new();
    let mut owner = f.start();
    f.enable();
    let commit = f.commit("one.txt");
    let trunk = success(git(
        &f.root.join("authority.git"),
        &["rev-parse", "refs/heads/trunk"],
    ));
    receipt(f.push(&["HEAD:refs/for/main"]));
    let before = f.submissions();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].commit(), commit);
    let id = before[0].id();
    let retained = format!("refs/mai/changes/{}/{}", id.change_id, id.generation);
    assert_eq!(
        success(git(
            &f.root.join("authority.git"),
            &["rev-parse", &retained]
        )),
        commit
    );
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    let _restarted = f.start();
    success(f.push(&["HEAD:refs/for/main"]));
    assert_eq!(f.submissions(), before);
    assert_eq!(
        success(git(
            &f.root.join("authority.git"),
            &["rev-parse", "refs/heads/trunk"]
        )),
        trunk
    );
}

#[test]
fn unavailable_owner_and_mixed_forbidden_push_do_not_admit() {
    let f = Fixture::new();
    let owner = f.start();
    f.enable();
    drop(owner);
    f.commit("one.txt");
    assert!(!f.push(&["HEAD:refs/for/main"]).status.success());
    assert!(f.submissions().is_empty());
    let _owner = f.start();
    let trunk = success(git(
        &f.root.join("authority.git"),
        &["rev-parse", "refs/heads/trunk"],
    ));
    assert!(
        !f.push(&["HEAD:refs/for/main", "HEAD:refs/heads/trunk"])
            .status
            .success()
    );
    assert!(
        f.submissions().is_empty(),
        "partially admitted rejected multi-ref push"
    );
    assert_eq!(
        success(git(
            &f.root.join("authority.git"),
            &["rev-parse", "refs/heads/trunk"]
        )),
        trunk
    );
}

#[test]
fn pending_submitted_parent_is_explicit_dependency_but_unknown_chain_rejects() {
    let f = Fixture::new();
    let _owner = f.start();
    f.enable();
    f.commit("parent.txt");
    success(f.push(&["HEAD:refs/for/main"]));
    let parent = f.submissions()[0].id().clone();
    let child = f.commit("child.txt");
    success(f.push(&["HEAD:refs/for/main"]));
    let all = f.submissions();
    assert_eq!(all.len(), 2);
    let submitted_child = all.iter().find(|s| s.commit() == child).unwrap();
    assert_eq!(submitted_child.dependencies(), &[parent]);
    f.commit("unsubmitted.txt");
    f.commit("unknown-chain.txt");
    assert!(!f.push(&["HEAD:refs/for/main"]).status.success());
    assert_eq!(f.submissions(), all);
}

#[cfg(unix)]
#[test]
fn disconnected_registration_response_retries_same_durable_identity() {
    use std::{io::Write, os::unix::net::UnixStream};
    let f = Fixture::new();
    let _owner = f.start();
    f.enable();
    let commit = f.commit("lost-reply.txt");
    // Import objects without submitting: this isolates the IPC acknowledgment
    // window from receive-pack's separately tested transport ordering.
    success(
        Command::new("git")
            .arg("--git-dir")
            .arg(f.root.join("authority.git"))
            .arg("fetch")
            .arg(&f.client)
            .arg("HEAD:refs/qa/import")
            .output()
            .unwrap(),
    );
    let request = serde_json::json!({"protocol_version":1,"action":{"register":{"agent":"qa","commit":commit}}});
    let mut socket = UnixStream::connect(f.root.join(".mai/engine.sock")).unwrap();
    writeln!(socket, "{request}").unwrap();
    drop(socket); // Complete request, intentionally no response read or ACK.
    let deadline = Instant::now() + Duration::from_secs(10);
    while f.submissions().is_empty() {
        assert!(
            Instant::now() < deadline,
            "disconnected registration was never persisted"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let before = f.submissions();
    success(f.push(&["HEAD:refs/for/main"]));
    assert_eq!(f.submissions(), before);
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].commit(), commit);
}

#[test]
fn rejected_scope_leaves_no_registered_or_retained_submission() {
    for path in [".env", "oversized.bin"] {
        let f = Fixture::new();
        let _owner = f.start();
        f.enable();
        let data = if path == ".env" {
            vec![b'x'; 12]
        } else {
            vec![b'x'; 65537]
        };
        std::fs::write(f.client.join(path), data).unwrap();
        success(git(&f.client, &["add", path]));
        success(git(&f.client, &["commit", "-m", "rejected fixture"]));
        let output = f.push(&["HEAD:refs/for/main"]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(if path == ".env" {
                "blocked receive path"
            } else {
                "per-file byte limit exceeded"
            }),
            "wrong rejection: {stderr}"
        );
        assert!(f.submissions().is_empty());
        assert!(
            success(git(
                &f.root.join("authority.git"),
                &["for-each-ref", "--format=%(refname)", "refs/mai/changes/"]
            ))
            .is_empty()
        );
    }
}

#[test]
fn orphan_retention_retry_and_missing_received_note_rebuild_converge() {
    let f = Fixture::new();
    let mut owner = f.start();
    f.enable();
    let commit = f.commit("orphan.txt");
    let retained = format!("refs/mai/changes/qa-{commit}/1");
    // Reproduce the durable prefix of registration before its database write.
    success(
        Command::new("git")
            .arg("--git-dir")
            .arg(f.root.join("authority.git"))
            .arg("fetch")
            .arg(&f.client)
            .arg(format!("HEAD:{retained}"))
            .output()
            .unwrap(),
    );
    assert!(f.submissions().is_empty());
    success(f.push(&["HEAD:refs/for/main"]));
    let before = f.submissions();
    assert_eq!(before.len(), 1);
    let repo = f.root.join("authority.git");
    let note = success(git(&repo, &["notes", "--ref=mai-objects", "show", &commit]));
    success(git(&repo, &["update-ref", "-d", "refs/notes/mai-objects"]));
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    let mut restarted = f.start();
    assert_eq!(
        success(git(&repo, &["notes", "--ref=mai-objects", "show", &commit])),
        note
    );
    assert_eq!(f.submissions(), before);
    restarted.0.kill().unwrap();
    restarted.0.wait().unwrap();
    let _again = f.start();
    assert_eq!(
        success(git(&repo, &["notes", "--ref=mai-objects", "show", &commit])),
        note
    );
    assert_eq!(f.submissions(), before);
}

#[test]
fn changed_file_count_limit_rejects_before_retention() {
    let f = Fixture::new();
    let _owner = f.start();
    f.enable();
    for index in 0..17 {
        std::fs::write(f.client.join(format!("file-{index}.txt")), "tiny").unwrap();
    }
    success(git(&f.client, &["add", "."]));
    success(git(&f.client, &["commit", "-m", "too many files"]));
    let output = f.push(&["HEAD:refs/for/main"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("changed-file limit exceeded"));
    assert!(f.submissions().is_empty());
    assert!(
        success(git(
            &f.root.join("authority.git"),
            &["for-each-ref", "--format=%(refname)", "refs/mai/changes/"]
        ))
        .is_empty()
    );
}

#[test]
fn incoming_pack_limit_rejects_before_engine_admission() {
    let f = Fixture::new();
    let _owner = f.start();
    success(cli(
        &f.root,
        &[
            "receive",
            "enable",
            "--max-input-bytes",
            "1024",
            "--max-file-bytes",
            "65536",
            "--max-changed-files",
            "16",
        ],
    ));
    // Deterministic incompressible-enough fixture; no network or random-source dependency.
    let mut state = 1_u32;
    let data: Vec<u8> = (0..32768)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    std::fs::write(f.client.join("pack.bin"), data).unwrap();
    success(git(&f.client, &["add", "pack.bin"]));
    success(git(&f.client, &["commit", "-m", "pack exceeds input cap"]));
    let output = f.push(&["HEAD:refs/for/main"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("maximum allowed size"),
        "wrong rejection: {stderr}"
    );
    assert!(f.submissions().is_empty());
    assert!(
        success(git(
            &f.root.join("authority.git"),
            &["for-each-ref", "--format=%(refname)", "refs/mai/changes/"]
        ))
        .is_empty()
    );
}
