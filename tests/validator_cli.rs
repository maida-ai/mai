use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mai"))
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}
fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}
#[test]
fn manual_validator_preserves_json_and_shell_failure_categories() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    fs::write(
        source.join("gate.sh"),
        "test ! -f \"$MAI_CANDIDATE_ROOT/fail\"\n",
    )
    .unwrap();
    fs::write(
        source.join("mai.toml"),
        r#"version = 1
[check]
program = "sh"
script = "gate.sh"
timeout_ms = 5000
output_tail_bytes = 1024
[validators]
slots = 1
port_base = 31000
ports_per_slot = 10
"#,
    )
    .unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "policy"]);
    let project = temp.path().join("project");
    assert!(
        cli(&[
            "init",
            project.to_str().unwrap(),
            "--from",
            source.to_str().unwrap()
        ])
        .status
        .success()
    );
    let repo = project.join("authority.git");
    let base = git(&repo, &["rev-parse", "trunk"]);
    let passed = cli(&["validator", "run", project.to_str().unwrap(), &base]);
    assert!(
        passed.status.success(),
        "{}",
        String::from_utf8_lossy(&passed.stderr)
    );
    assert_eq!(json(&passed)["outcome"], "success");
    fs::write(source.join("gate.sh"), "exit 0\n").unwrap();
    fs::write(source.join("fail"), "failure input").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "candidate weakens gate"]);
    git(
        &repo,
        &[
            "fetch",
            source.to_str().unwrap(),
            "HEAD:refs/mai/candidates/test",
        ],
    );
    let candidate = git(&repo, &["rev-parse", "refs/mai/candidates/test"]);
    let pending = cli(&["validator", "run", project.to_str().unwrap(), &candidate]);
    assert_eq!(pending.status.code(), Some(6));
    assert_eq!(json(&pending)["outcome"], "approval_required");
    let denied = cli(&[
        "approve",
        project.to_str().unwrap(),
        &candidate,
        "--actor",
        "maintainer",
        "--reason",
        "reviewed and denied",
        "--deny",
    ]);
    assert!(denied.status.success());
    let denial = cli(&["validator", "run", project.to_str().unwrap(), &candidate]);
    assert_eq!(denial.status.code(), Some(3));
    assert_eq!(json(&denial)["outcome"], "policy_denied");
    assert!(
        cli(&[
            "approve",
            project.to_str().unwrap(),
            &candidate,
            "--actor",
            "maintainer",
            "--reason",
            "run normal gate"
        ])
        .status
        .success()
    );
    let failed = cli(&["validator", "run", project.to_str().unwrap(), &candidate]);
    assert_eq!(failed.status.code(), Some(3));
    assert_eq!(json(&failed)["outcome"], "failure");
    assert_eq!(git(&repo, &["rev-parse", "trunk"]), base);
}

#[test]
fn declared_helper_executes_from_trusted_checkout_even_after_candidate_approval() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    fs::write(source.join("gate.sh"), "sh helper.sh\n").unwrap();
    fs::write(source.join("helper.sh"), "exit 17\n").unwrap();
    fs::write(
        source.join("mai.toml"),
        r#"version=1
[check]
program="sh"
script="gate.sh"
executable_dependencies=["helper.sh"]
timeout_ms=1000
output_tail_bytes=1024
[validators]
slots=1
port_base=32000
ports_per_slot=10
"#,
    )
    .unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "policy"]);
    let project = temp.path().join("authority");
    assert!(
        cli(&[
            "init",
            project.to_str().unwrap(),
            "--from",
            source.to_str().unwrap()
        ])
        .status
        .success()
    );
    fs::write(source.join("helper.sh"), "exit 0\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "replace helper"]);
    let repo = project.join("authority.git");
    git(
        &repo,
        &[
            "fetch",
            source.to_str().unwrap(),
            "HEAD:refs/mai/candidates/helper",
        ],
    );
    let candidate = git(&repo, &["rev-parse", "refs/mai/candidates/helper"]);
    assert_eq!(
        json(&cli(&[
            "validator",
            "run",
            project.to_str().unwrap(),
            &candidate
        ]))["outcome"],
        "approval_required"
    );
    assert!(
        cli(&[
            "approve",
            project.to_str().unwrap(),
            &candidate,
            "--actor",
            "maintainer",
            "--reason",
            "reviewed"
        ])
        .status
        .success()
    );
    let result = cli(&["validator", "run", project.to_str().unwrap(), &candidate]);
    assert_eq!(json(&result)["outcome"], "failure");
    assert_eq!(json(&result)["process"]["exit_code"], 17);
}

#[test]
fn bootstrap_is_bounded_reused_and_invalidated_when_output_disappears() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    fs::write(source.join("bootstrap.sh"), "if grep -q fail \"$MAI_CANDIDATE_ROOT/lock\"; then exit 8; fi; mkdir -p \"$MAI_CANDIDATE_ROOT/cache\"; echo ready > \"$MAI_CANDIDATE_ROOT/cache/ready\"\n").unwrap();
    fs::write(source.join("gate.sh"), "test -f \"$MAI_CANDIDATE_ROOT/cache/ready\" || exit 7; if test -f \"$MAI_CANDIDATE_ROOT/remove\"; then rm -r \"$MAI_CANDIDATE_ROOT/cache\"; fi\n").unwrap();
    fs::write(
        source.join("mai.toml"),
        r#"version=1
[check]
program="sh"
script="gate.sh"
timeout_ms=1000
output_tail_bytes=1024
[validators]
slots=1
port_base=33000
ports_per_slot=10
caches=["cache"]
[validators.bootstrap]
program="sh"
script="bootstrap.sh"
inputs=["lock"]
timeout_ms=1000
output_tail_bytes=1024
"#,
    )
    .unwrap();
    fs::write(source.join("lock"), "valid").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "policy"]);
    let project = temp.path().join("authority");
    assert!(
        cli(&[
            "init",
            project.to_str().unwrap(),
            "--from",
            source.to_str().unwrap()
        ])
        .status
        .success()
    );
    let repo = project.join("authority.git");
    let base = git(&repo, &["rev-parse", "trunk"]);
    let first = json(&cli(&[
        "validator",
        "run",
        project.to_str().unwrap(),
        &base,
    ]));
    assert_eq!(first["outcome"], "success");
    assert!(first["bootstrap"].is_object());
    assert_eq!(first["bootstrap_reused"], false);
    let second = json(&cli(&[
        "validator",
        "run",
        project.to_str().unwrap(),
        &base,
    ]));
    assert_eq!(second["outcome"], "success");
    assert_eq!(second["bootstrap_reused"], true);
    fs::write(source.join("remove"), "remove cache").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "remove cached setup"]);
    git(
        &repo,
        &[
            "fetch",
            source.to_str().unwrap(),
            "HEAD:refs/mai/candidates/remove",
        ],
    );
    let remove = git(&repo, &["rev-parse", "refs/mai/candidates/remove"]);
    assert_eq!(
        json(&cli(&[
            "validator",
            "run",
            project.to_str().unwrap(),
            &remove
        ]))["outcome"],
        "success"
    );
    let rebuilt = json(&cli(&[
        "validator",
        "run",
        project.to_str().unwrap(),
        &base,
    ]));
    assert_eq!(rebuilt["outcome"], "success");
    assert_eq!(rebuilt["bootstrap_reused"], false);
    assert!(rebuilt["bootstrap"].is_object());
    fs::write(source.join("lock"), "fail").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "invalid bootstrap input"]);
    git(
        &repo,
        &[
            "fetch",
            source.to_str().unwrap(),
            "HEAD:refs/mai/candidates/bad-input",
        ],
    );
    let bad = git(&repo, &["rev-parse", "refs/mai/candidates/bad-input"]);
    assert!(
        cli(&[
            "approve",
            project.to_str().unwrap(),
            &bad,
            "--actor",
            "maintainer",
            "--reason",
            "exercise bootstrap failure"
        ])
        .status
        .success()
    );
    let failure = json(&cli(&["validator", "run", project.to_str().unwrap(), &bad]));
    assert_eq!(failure["outcome"], "failure");
    assert!(failure["process"].is_null());
    assert_eq!(failure["bootstrap"]["exit_code"], 8);
    let reverted = json(&cli(&[
        "validator",
        "run",
        project.to_str().unwrap(),
        &base,
    ]));
    assert_eq!(reverted["outcome"], "success");
    assert_eq!(reverted["bootstrap_reused"], false);
    assert!(reverted["bootstrap"].is_object());
}

#[test]
fn validator_never_records_original_tree_success_after_tracked_mutation() {
    for phase in ["bootstrap", "gate", "bootstrap-hidden-index"] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        git(&source, &["init"]);
        fs::write(source.join("input"), "bad\n").unwrap();
        let mutation = "printf 'good\\n' > \"$MAI_CANDIDATE_ROOT/input\"\n";
        let bootstrap = match phase {
            "bootstrap" => mutation.to_owned(),
            "bootstrap-hidden-index" => format!(
                "{mutation}git -C \"$MAI_CANDIDATE_ROOT\" add input\ngit -C \"$MAI_CANDIDATE_ROOT\" update-index --assume-unchanged input\n"
            ),
            _ => ":\n".to_owned(),
        };
        fs::write(source.join("bootstrap.sh"), bootstrap).unwrap();
        let gate = if phase == "gate" {
            format!("{mutation}grep -q good \"$MAI_CANDIDATE_ROOT/input\"\n")
        } else {
            "grep -q good \"$MAI_CANDIDATE_ROOT/input\"\n".to_owned()
        };
        fs::write(source.join("gate.sh"), gate).unwrap();
        fs::write(
            source.join("mai.toml"),
            r#"version=1
[check]
program="sh"
script="gate.sh"
timeout_ms=5000
output_tail_bytes=1024
[validators]
slots=1
port_base=34000
ports_per_slot=10
[validators.bootstrap]
program="sh"
script="bootstrap.sh"
timeout_ms=5000
output_tail_bytes=1024
"#,
        )
        .unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-m", "trusted policy"]);
        let project = temp.path().join("project");
        assert!(
            cli(&[
                "init",
                project.to_str().unwrap(),
                "--from",
                source.to_str().unwrap()
            ])
            .status
            .success()
        );
        let repo = project.join("authority.git");
        let candidate = git(&repo, &["rev-parse", "trunk"]);
        let tree = git(&repo, &["rev-parse", "trunk^{tree}"]);
        let output = cli(&["validator", "run", project.to_str().unwrap(), &candidate]);
        let result = json(&output);
        assert_eq!(result["outcome"], "failure", "{phase}: {result}");
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(result["binding"]["tree"], tree);
        assert!(
            result["diagnostic"]
                .as_str()
                .unwrap()
                .contains("tracked candidate")
        );
        if phase == "gate" {
            assert_eq!(result["process"]["outcome"], "success");
        } else {
            assert!(
                result["process"].is_null(),
                "mutating bootstrap must prevent gate execution"
            );
            assert!(!project.join(".mai/validator-bootstrap-0.json").exists());
        }
        // Failed validation evidence must persist, without changing trunk.
        assert_eq!(git(&repo, &["rev-parse", "trunk"]), candidate);
    }
}

#[test]
fn approval_administration_obeys_engine_ownership_with_and_without_daemon() {
    use fs2::FileExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    fs::write(source.join("gate.sh"), "exit 0\n").unwrap();
    fs::write(source.join("mai.toml"), "version=1\n[check]\nprogram=\"sh\"\nscript=\"gate.sh\"\ntimeout_ms=1000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=33000\nports_per_slot=10\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "trusted policy"]);
    let project = temp.path().join("project");
    assert!(
        cli(&[
            "init",
            project.to_str().unwrap(),
            "--from",
            source.to_str().unwrap()
        ])
        .status
        .success()
    );
    let candidate = git(&project.join("authority.git"), &["rev-parse", "trunk"]);
    let approve = || {
        cli(&[
            "approve",
            project.to_str().unwrap(),
            &candidate,
            "--actor",
            "maintainer",
            "--reason",
            "fixture administration",
        ])
    };
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(project.join(".mai/engine.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(
        !approve().status.success(),
        "an owner without IPC must not permit an unlocked approval write"
    );
    assert!(!project.join(".mai/gates.db").exists());
    FileExt::unlock(&lock).unwrap();
    let standalone = approve();
    assert!(
        standalone.status.success(),
        "{}",
        String::from_utf8_lossy(&standalone.stderr)
    );
    assert!(
        !project.join(".mai/engine.pid").exists(),
        "standalone administration must not leak a daemon"
    );
    struct Stop(std::path::PathBuf);
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = cli(&["stop", self.0.to_str().unwrap()]);
        }
    }
    let _stop = Stop(project.clone());
    assert!(cli(&["status", project.to_str().unwrap()]).status.success());
    let routed = approve();
    assert!(
        routed.status.success(),
        "{}",
        String::from_utf8_lossy(&routed.stderr)
    );
    assert_eq!(json(&routed)["decision"], "approved");
    assert_eq!(json(&routed)["binding"], json(&standalone)["binding"]);
}
