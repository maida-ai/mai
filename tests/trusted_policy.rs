use mai::{
    approval::{ApprovalStore, Decision},
    authority::Authority,
    policy::TrustedPolicy,
};
use std::{fs, path::Path, process::Command};
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
const POLICY: &str = r#"version=1
[check]
program="sh"
script="gate.sh"
timeout_ms=1000
output_tail_bytes=1024
[validators]
slots=1
port_base=24000
ports_per_slot=10
[policy]
protected=["protected/**"]
"#;
fn fixture() -> (tempfile::TempDir, Authority, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    fs::write(source.join("mai.toml"), POLICY).unwrap();
    fs::write(source.join("gate.sh"), "exit 9\n").unwrap();
    fs::write(source.join("input"), "base").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "base"]);
    let authority = Authority::init(&temp.path().join("authority"), Some(&source)).unwrap();
    (temp, authority, source)
}
fn candidate(authority: &Authority, source: &Path) -> String {
    git(source, &["add", "."]);
    git(source, &["commit", "-m", "candidate"]);
    let oid = git(source, &["rev-parse", "HEAD"]);
    git(
        &authority.git_dir(),
        &[
            "-c",
            "protocol.file.allow=always",
            "fetch",
            source.to_str().unwrap(),
            &oid,
        ],
    );
    oid
}
#[test]
fn candidate_policy_narrowing_does_not_replace_trusted_controls_and_approval_is_exact() {
    let (_temp, authority, source) = fixture();
    fs::write(
        source.join("mai.toml"),
        POLICY.replace("protected/**", "other/**"),
    )
    .unwrap();
    fs::write(source.join("gate.sh"), "exit 0\n").unwrap();
    fs::create_dir(source.join("protected")).unwrap();
    fs::write(source.join("protected/file"), "value").unwrap();
    let oid = candidate(&authority, &source);
    let trusted = TrustedPolicy::load(&authority, &oid).unwrap();
    assert_eq!(
        fs::read_to_string(trusted.root().join("gate.sh")).unwrap(),
        "exit 9\n"
    );
    assert_eq!(trusted.protected_changes.len(), 3);
    let mut store = ApprovalStore::open(authority.root()).unwrap();
    assert!(store.decision(&trusted.binding).unwrap().is_none());
    assert!(
        store
            .record(&trusted.binding, Decision::Approved, "", "reason")
            .is_err()
    );
    store
        .record(&trusted.binding, Decision::Denied, "maintainer", "reject")
        .unwrap();
    assert_eq!(
        store.decision(&trusted.binding).unwrap().unwrap().decision,
        Decision::Denied
    );
    store
        .record(
            &trusted.binding,
            Decision::Approved,
            "maintainer",
            "reviewed",
        )
        .unwrap();
    assert_eq!(
        store.decision(&trusted.binding).unwrap().unwrap().decision,
        Decision::Approved
    );
    for field in ["candidate", "tree", "base", "policy"] {
        let mut binding = trusted.binding.clone();
        match field {
            "candidate" => binding.candidate = "a".repeat(40),
            "tree" => binding.tree = "a".repeat(40),
            "base" => binding.base = "a".repeat(40),
            _ => binding.policy = "a".repeat(40),
        };
        assert!(store.decision(&binding).unwrap().is_none());
    }
    let mut own = trusted.binding.clone();
    own.submitted_identity = Some("agent".into());
    assert!(
        store
            .record(&own, Decision::Approved, "agent", "self")
            .is_err()
    );
    assert!(store.decision(&own).unwrap().is_none());
}
#[test]
fn unsupported_trusted_policy_is_rejected_without_candidate_override() {
    let (_temp, authority, source) = fixture();
    fs::write(
        source.join("mai.toml"),
        POLICY.replace("version=1", "version=2"),
    )
    .unwrap();
    let oid = candidate(&authority, &source);
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &oid],
    );
    assert!(TrustedPolicy::load(&authority, &oid).is_err());
    fs::write(
        source.join("mai.toml"),
        format!("{POLICY}\nunsupported=true\n"),
    )
    .unwrap();
    let oid = candidate(&authority, &source);
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &oid],
    );
    assert!(TrustedPolicy::load(&authority, &oid).is_err());
}
#[cfg(unix)]
#[test]
fn trusted_script_symlinks_and_renames_are_fail_closed() {
    use std::os::unix::fs::symlink;
    let (_temp, authority, source) = fixture();
    fs::rename(source.join("gate.sh"), source.join("moved.sh")).unwrap();
    symlink("moved.sh", source.join("gate.sh")).unwrap();
    let oid = candidate(&authority, &source);
    let trusted = TrustedPolicy::load(&authority, &oid).unwrap();
    assert_eq!(trusted.protected_changes.len(), 1);
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &oid],
    );
    assert!(TrustedPolicy::load(&authority, &oid).is_err());
}
