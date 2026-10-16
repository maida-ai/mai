use mai::{authority::Authority, workspace};
use std::{fs, path::Path, process::Command};
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
#[test]
fn independent_workspace_has_agent_remote_and_preserves_existing_work() {
    let temp = tempfile::tempdir().unwrap();
    let authority = Authority::init(&temp.path().join("project"), None).unwrap();
    let destination = temp.path().join("alice");
    let result = workspace::create(authority.root(), "alice", &destination, false).unwrap();
    assert_eq!(result.agent, "alice");
    assert!(result.bootstrap.is_none());
    assert!(!destination.join(".git/objects/info/alternates").exists());
    assert_eq!(git(&destination, &["config", "mai.agent"]), "alice");
    assert_eq!(git(&destination, &["remote"]), "mai");
    fs::write(destination.join("untracked"), "precious").unwrap();
    assert!(workspace::create(authority.root(), "alice", &destination, false).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("untracked")).unwrap(),
        "precious"
    );
    assert!(
        workspace::create(authority.root(), "../bad", &temp.path().join("bad"), false).is_err()
    );
    assert!(!temp.path().join("bad").exists());
}
#[cfg(unix)]
#[test]
fn non_utf8_workspace_destination_round_trips_json() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let authority = Authority::init(&temp.path().join("project"), None).unwrap();
    let destination = temp.path().join(OsString::from_vec(b"clone-\xff".to_vec()));
    let result = workspace::create(authority.root(), "alice", &destination, true).unwrap();
    let json = serde_json::to_value(&result).unwrap();
    let path: mai::native_path::NativePath =
        serde_json::from_value(json["destination"].clone()).unwrap();
    assert_eq!(path.as_path(), destination);
}
#[cfg(unix)]
fn configured_project(failing: bool) -> (tempfile::TempDir, Authority) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    git(&source, &["config", "user.name", "fixture"]);
    git(&source, &["config", "user.email", "fixture@localhost"]);
    fs::write(
        source.join("bootstrap.sh"),
        if failing {
            "printf 'failed setup' >&2\nexit 7\n"
        } else {
            "cd \"$MAI_CANDIDATE_ROOT\"\ntest -n \"$TMPDIR\" && printf 'ready' > environment\n"
        },
    )
    .unwrap();
    fs::write(source.join("input.lock"), "one").unwrap();
    fs::write(
        source.join("mai.toml"),
        r#"version = 1
[check]
program = "sh"
script = "bootstrap.sh"
timeout_ms = 5000
output_tail_bytes = 512
[validators]
slots = 1
port_base = 20000
ports_per_slot = 10
[workspaces.bootstrap]
program = "sh"
script = "bootstrap.sh"
inputs = ["input.lock"]
tools = ["git"]
timeout_ms = 5000
output_tail_bytes = 512
"#,
    )
    .unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "configured fixture"]);
    let authority = Authority::init(&temp.path().join("project"), Some(&source)).unwrap();
    (temp, authority)
}
#[cfg(unix)]
#[test]
fn automatic_bootstrap_skip_failure_and_independent_environments() {
    use mai::process::ProcessOutcome;
    let (temp, authority) = configured_project(false);
    let alice = temp.path().join("alice");
    let bob = temp.path().join("bob");
    let first = workspace::create(authority.root(), "alice", &alice, false).unwrap();
    assert_eq!(first.bootstrap.unwrap().outcome, ProcessOutcome::Success);
    workspace::create(authority.root(), "bob", &bob, false).unwrap();
    fs::write(alice.join("environment"), "changed").unwrap();
    assert_eq!(
        fs::read_to_string(bob.join("environment")).unwrap(),
        "ready"
    );
    let skipped = temp.path().join("skipped");
    let result = workspace::create(authority.root(), "skip", &skipped, true).unwrap();
    assert!(result.bootstrap_skipped && result.bootstrap.is_none());
    assert!(!skipped.join("environment").exists());
    let (bad_temp, bad) = configured_project(true);
    let failed = bad_temp.path().join("failed");
    let result = workspace::create(bad.root(), "fail", &failed, false).unwrap();
    let run = result.bootstrap.unwrap();
    assert_eq!(run.outcome, ProcessOutcome::Failure);
    assert!(run.stderr_tail.contains("failed setup"));
    assert!(failed.join(".git").exists());
    assert!(failed.join(".mai-workspace-incomplete").exists());
    fs::write(failed.join("precious"), "preserve").unwrap();
    assert!(workspace::create(bad.root(), "fail", &failed, false).is_err());
    assert!(failed.join("precious").exists());
}
#[cfg(unix)]
#[test]
fn bootstrap_fingerprint_tracks_inputs_tools_and_trusted_control() {
    use mai::{bootstrap, policy::TrustedPolicy};
    let (temp, authority) = configured_project(false);
    let destination = temp.path().join("alice");
    let result = workspace::create(authority.root(), "alice", &destination, true).unwrap();
    let trusted = TrustedPolicy::load(&authority, &result.base).unwrap();
    let command = trusted.config.workspaces.bootstrap.as_ref().unwrap();
    let initial = bootstrap::fingerprint(&trusted, command, &destination).unwrap();
    fs::write(destination.join("bootstrap.sh"), "exit 88\n").unwrap();
    assert_eq!(
        initial,
        bootstrap::fingerprint(&trusted, command, &destination).unwrap()
    );
    // Candidate script replacement cannot replace the trusted bootstrap script.
    assert_eq!(
        bootstrap::run(&trusted, command, &destination, Default::default())
            .unwrap()
            .outcome,
        mai::process::ProcessOutcome::Success
    );
    fs::write(destination.join("input.lock"), "two").unwrap();
    assert_ne!(
        initial,
        bootstrap::fingerprint(&trusted, command, &destination).unwrap()
    );
    fs::remove_file(destination.join("input.lock")).unwrap();
    std::os::unix::fs::symlink("bootstrap.sh", destination.join("input.lock")).unwrap();
    assert!(bootstrap::fingerprint(&trusted, command, &destination).is_err());
}
#[test]
fn empty_existing_destination_and_symlink_are_never_reused() {
    let temp = tempfile::tempdir().unwrap();
    let authority = Authority::init(&temp.path().join("project"), None).unwrap();
    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    assert!(workspace::create(authority.root(), "alice", &existing, true).is_err());
    assert!(fs::read_dir(&existing).unwrap().next().is_none());
    #[cfg(unix)]
    {
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&existing, &alias).unwrap();
        assert!(workspace::create(authority.root(), "alice", &alias, true).is_err());
        assert!(fs::read_dir(&existing).unwrap().next().is_none());
    }
}
#[cfg(unix)]
#[test]
fn changed_tool_binary_with_same_version_invalidates_bootstrap() {
    use mai::{bootstrap, policy::TrustedPolicy};
    use std::os::unix::fs::PermissionsExt;
    let (temp, authority) = configured_project(false);
    let destination = temp.path().join("alice");
    let result = workspace::create(authority.root(), "alice", &destination, true).unwrap();
    let trusted = TrustedPolicy::load(&authority, &result.base).unwrap();
    let mut command = trusted
        .config
        .workspaces
        .bootstrap
        .as_ref()
        .unwrap()
        .clone();
    let tool = temp.path().join("fixture-tool");
    fs::write(&tool, "#!/bin/sh\necho version-one\n").unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    command.tools = vec![tool.to_str().unwrap().into()];
    let before = bootstrap::fingerprint(&trusted, &command, &destination).unwrap();
    fs::write(&tool, "#!/bin/sh\necho version-one\n# changed executable\n").unwrap();
    assert_ne!(
        before,
        bootstrap::fingerprint(&trusted, &command, &destination).unwrap()
    );
}
