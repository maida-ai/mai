//! Independent actual-ledger continuation and source-preservation qualification.
#[path = "continuation_support/mod.rs"]
mod support;
use std::fs;
use support::*;

#[test]
fn accepted_parent_is_excluded_and_only_local_suffix_replays_on_later_trunk() {
    let f = Fixture::new();
    let before = snapshot(&f.source);
    let result = json(f.continue_to(&f.destination));
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["agent"], "alice");
    assert_eq!(result["base_trunk"], f.trunk);
    assert_eq!(result["submitted_commit"], f.submitted);
    assert_eq!(result["landed_commit"], f.landed);
    assert_eq!(result["replayed"].as_array().unwrap().len(), 1);
    assert_eq!(result["replayed"][0]["source_commit"], f.suffix);
    let destination_head = git(&f.destination, &["rev-parse", "HEAD"]);
    assert_eq!(
        result["replayed"][0]["destination_commit"],
        destination_head
    );
    assert_eq!(git(&f.destination, &["rev-parse", "HEAD^"]), f.trunk);
    assert_eq!(
        fs::read(f.destination.join("accepted.txt")).unwrap(),
        b"later accepted trunk edit\n"
    );
    assert_eq!(
        fs::read(f.destination.join("local.txt")).unwrap(),
        b"local suffix\n"
    );
    assert_eq!(
        fs::read(f.destination.join("bob.txt")).unwrap(),
        b"independent trunk content\n"
    );
    assert!(!f.destination.join("ignored").exists());
    assert!(!f.destination.join(".git/objects/info/alternates").exists());
    assert!(!f.destination.join(".mai-continuation-incomplete").exists());
    assert!(
        !git_output(&f.destination, &["config", "qa.preserve"])
            .status
            .success()
    );
    assert_eq!(snapshot(&f.source), before);
    f.assert_authority_unchanged();
    let retained = snapshot(&f.destination);
    assert!(!f.continue_to(&f.destination).status.success());
    assert_eq!(snapshot(&f.destination), retained);
    assert_eq!(snapshot(&f.source), before);
}

#[test]
fn replay_conflict_preserves_owned_destination_and_source_for_corrected_retry() {
    let f = Fixture::new();
    f.commit_local(
        "accepted.txt",
        "conflicting subsequent local edit\n",
        "local overlapping suffix",
    );
    let before = snapshot(&f.source);
    let output = f.continue_to(&f.destination);
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(f.destination.join(".mai-continuation-incomplete").is_file());
    assert_eq!(snapshot(&f.source), before);
    f.assert_authority_unchanged();
    fs::write(
        f.destination.join("precious-conflict-evidence"),
        "retain failed copy\n",
    )
    .unwrap();
    let failed = snapshot(&f.destination);
    assert!(!f.continue_to(&f.destination).status.success());
    assert_eq!(snapshot(&f.destination), failed);
    assert_eq!(snapshot(&f.source), before);
    // Correction is an explicit operator edit in the old source; continue
    // itself never rewrites or resolves that source on the user's behalf.
    git(&f.source, &["reset", "--hard", &f.suffix]);
    let corrected = snapshot(&f.source);
    let new_destination = f.temp.path().join("corrected-continuation");
    let result = json(f.continue_to(&new_destination));
    assert_eq!(result["replayed"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot(&f.source), corrected);
    assert_eq!(snapshot(&f.destination), failed);
    assert_eq!(
        fs::read(new_destination.join("accepted.txt")).unwrap(),
        b"later accepted trunk edit\n"
    );
    f.assert_authority_unchanged();
}

#[test]
fn no_local_suffix_creates_only_the_selected_accepted_trunk() {
    let f = Fixture::new();
    git(&f.source, &["reset", "--hard", &f.submitted]);
    let before = snapshot(&f.source);
    let result = json(f.continue_to(&f.destination));
    assert!(result["replayed"].as_array().unwrap().is_empty());
    assert_eq!(git(&f.destination, &["rev-parse", "HEAD"]), f.trunk);
    assert_eq!(snapshot(&f.source), before);
    assert!(!f.destination.join("ignored").exists());
    f.assert_authority_unchanged();
}

#[test]
fn default_bootstrap_uses_pinned_control_after_replay_and_preserves_source() {
    let f = Fixture::with_bootstrap(Some(
        "test \"$(cat \"$MAI_CANDIDATE_ROOT/local.txt\")\" = 'local suffix' || exit 34\nprintf 'trusted bootstrap saw replayed work' > \"$MAI_CANDIDATE_ROOT/bootstrapped\"\n",
    ));
    f.commit_local(
        "bootstrap.sh",
        "printf candidate > \"$MAI_CANDIDATE_ROOT/candidate-control-ran\"\nexit 91\n",
        "candidate changes bootstrap control",
    );
    let before = snapshot(&f.source);
    let result = json(f.continue_default_to(&f.destination));
    assert_eq!(result["bootstrap"]["outcome"], "success");
    assert_eq!(result["bootstrap_skipped"], false);
    assert_eq!(result["replayed"].as_array().unwrap().len(), 2);
    assert_eq!(
        fs::read(f.destination.join("bootstrapped")).unwrap(),
        b"trusted bootstrap saw replayed work"
    );
    assert!(!f.destination.join("candidate-control-ran").exists());
    assert!(
        fs::read_to_string(f.destination.join("bootstrap.sh"))
            .unwrap()
            .contains("exit 91")
    );
    assert!(!f.destination.join(".mai-continuation-incomplete").exists());
    assert_eq!(snapshot(&f.source), before);
    f.assert_authority_unchanged();
}

#[test]
fn default_bootstrap_failure_retains_owned_copy_and_structured_diagnostics() {
    let f = Fixture::with_bootstrap(Some(
        "test \"$(cat \"$MAI_CANDIDATE_ROOT/local.txt\")\" = 'local suffix' || exit 34\nprintf 'bootstrap fixture failed after replay' >&2\nexit 23\n",
    ));
    let before = snapshot(&f.source);
    let output = f.continue_default_to(&f.destination);
    assert!(!output.status.success());
    assert!(f.destination.join(".mai-continuation-incomplete").is_file());
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(f.destination.join(".mai-continuation-failure.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["kind"], "bootstrap_failed");
    assert_eq!(evidence["result"]["outcome"], "failure");
    assert_eq!(evidence["result"]["exit_code"], 23);
    assert!(
        evidence["result"]["stderr_tail"]
            .as_str()
            .unwrap()
            .contains("bootstrap fixture failed after replay")
    );
    assert_eq!(snapshot(&f.source), before);
    let failed = snapshot(&f.destination);
    assert!(!f.continue_default_to(&f.destination).status.success());
    assert_eq!(snapshot(&f.destination), failed);
    assert_eq!(snapshot(&f.source), before);
    f.assert_authority_unchanged();
}

#[cfg(unix)]
#[test]
fn actual_copy_and_replay_interruption_preserve_owned_evidence_and_refuse_adoption() {
    use std::{
        ffi::OsString,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    struct Release(std::path::PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(self.0.join("release"), b"release");
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.0.join("ready").exists()
                && !self.0.join("done").exists()
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    fn quote(value: &std::path::Path) -> String {
        format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"))
    }
    let old_path = std::env::var_os("PATH").unwrap();
    let real_git = std::env::split_paths(&old_path)
        .map(|entry| entry.join("git"))
        .find(|path| path.is_file())
        .unwrap()
        .canonicalize()
        .unwrap();
    for phase in ["clone", "cherry-pick"] {
        let f = Fixture::new();
        let control = f.temp.path().join("interruption-control");
        let tools = f.temp.path().join("fixture-tools");
        fs::create_dir(&control).unwrap();
        fs::create_dir(&tools).unwrap();
        let _release = Release(control.clone());
        let wrapper = tools.join("git");
        fs::write(&wrapper, format!("#!/bin/sh\nselected=0\nfor arg do\n if test \"$arg\" = '{phase}'; then selected=1; fi\ndone\n{} \"$@\"\nresult=$?\nif test \"$selected\" = 1 && test \"$result\" = 0; then\n printf ready > {}/ready\n i=0\n while ! test -f {}/release; do\n  i=$((i+1))\n  if test \"$i\" -ge 1500; then exit 125; fi\n  sleep 0.02\n done\n printf done > {}/done\nfi\nexit \"$result\"\n", quote(&real_git), quote(&control), quote(&control), quote(&control))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let mut paths = vec![tools];
        paths.extend(std::env::split_paths(&old_path));
        let instrumented_path: OsString = std::env::join_paths(paths).unwrap();
        let log_path = f.temp.path().join("interrupted-continue.log");
        let log = fs::File::create(&log_path).unwrap();
        let before = snapshot(&f.source);
        let mut process = Process(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .arg("--json")
                .arg("continue")
                .arg(&f.project)
                .arg(&f.source)
                .arg(&f.destination)
                .args([
                    "--change",
                    &f.change,
                    "--generation",
                    "1",
                    "--skip-bootstrap",
                ])
                .env("PATH", instrumented_path)
                .stdout(Stdio::from(log.try_clone().unwrap()))
                .stderr(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !control.join("ready").exists() {
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "phase={phase}: {}",
                fs::read_to_string(&log_path).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "phase={phase}: {}",
                fs::read_to_string(&log_path).unwrap()
            );
            thread::sleep(Duration::from_millis(20));
        }
        // Only the fixture continuation client is killed. The isolated wrapper
        // has already finished its real Git operation and is then released.
        process.0.kill().unwrap();
        assert!(!process.0.wait().unwrap().success());
        fs::write(control.join("release"), b"release").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !control.join("done").exists() {
            assert!(
                Instant::now() < deadline,
                "fixture wrapper did not exit its pause"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            f.destination.join(".mai-continuation-incomplete").is_file(),
            "phase={phase}"
        );
        assert_eq!(snapshot(&f.source), before, "phase={phase}");
        let failed = snapshot(&f.destination);
        assert!(!f.continue_to(&f.destination).status.success());
        assert_eq!(snapshot(&f.destination), failed);
        let next = f.temp.path().join("retry-in-new-destination");
        let result = json(f.continue_to(&next));
        assert_eq!(result["replayed"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot(&f.source), before);
        assert_eq!(snapshot(&f.destination), failed);
        f.assert_authority_unchanged();
    }
}
