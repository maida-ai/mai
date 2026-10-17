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
    let spec = trusted
        .command_spec(&trusted.config.check, source.as_path())
        .unwrap();
    assert_eq!(spec.cwd, trusted.root());
    assert_eq!(
        spec.env.get(std::ffi::OsStr::new("MAI_POLICY_ROOT")),
        Some(&mai::policy::external_command_path(trusted.root()))
    );
    assert_eq!(
        spec.env.get(std::ffi::OsStr::new("MAI_CANDIDATE_ROOT")),
        Some(&mai::policy::external_command_path(&source))
    );
    assert_eq!(
        spec.args[0],
        mai::policy::external_command_path(&trusted.script(&trusted.config.check).unwrap())
    );

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

#[test]
fn lexical_policy_path_aliases_are_rejected_before_protection_matching() {
    for alias in [
        "scripts/./gate.sh",
        "scripts//gate.sh",
        "scripts/gate.sh/",
        "scripts/.",
    ] {
        assert!(
            mai::policy::relative_path(Path::new(alias)).is_err(),
            "accepted noncanonical path {alias:?}"
        );
    }
    for path in ["gate.sh", "scripts/gate.sh", "scripts/non-ascii-é"] {
        mai::policy::relative_path(Path::new(path)).unwrap();
    }
}

#[test]
fn aliases_cannot_enter_any_trusted_path_configuration_field() {
    let (_temp, authority, source) = fixture();
    fs::create_dir(source.join("scripts")).unwrap();
    fs::write(source.join("scripts/gate.sh"), "exit 0\n").unwrap();
    for alias in ["scripts/./gate.sh", "scripts//gate.sh"] {
        for field in [
            "script",
            "executable_dependencies",
            "inputs",
            "protected",
            "caches",
        ] {
            let policy = match field {
                "script" => POLICY.replace("script=\"gate.sh\"", &format!("script=\"{alias}\"")),
                "executable_dependencies" | "inputs" => POLICY.replace(
                    "script=\"gate.sh\"",
                    &format!("script=\"gate.sh\"\n{field}=[\"{alias}\"]"),
                ),
                "protected" => POLICY.replace("protected/**", alias),
                _ => POLICY.replace("slots=1", &format!("slots=1\ncaches=[\"{alias}\"]")),
            };
            fs::write(source.join("mai.toml"), policy).unwrap();
            let oid = candidate(&authority, &source);
            git(
                &authority.git_dir(),
                &["update-ref", "refs/heads/trunk", &oid],
            );
            assert!(
                TrustedPolicy::load(&authority, &oid).is_err(),
                "accepted {field} alias {alias:?}"
            );
        }
    }
}

#[test]
fn exact_git_tree_spelling_is_required_for_executable_policy() {
    let (_temp, authority, source) = fixture();
    fs::write(
        source.join("mai.toml"),
        POLICY.replace("gate.sh", "GATE.SH"),
    )
    .unwrap();
    let oid = candidate(&authority, &source);
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &oid],
    );
    let error = match TrustedPolicy::load(&authority, &oid) {
        Ok(_) => panic!("accepted case alias"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("trusted Git path"), "{error:#}");
}

#[test]
fn git_symlink_modes_reject_even_when_working_copy_is_a_regular_file() {
    let (_temp, authority, source) = fixture();
    let blob = git(&source, &["hash-object", "gate.sh"]);
    git(
        &source,
        &[
            "update-index",
            "--cacheinfo",
            &format!("120000,{blob},gate.sh"),
        ],
    );
    git(&source, &["commit", "-m", "symlink-mode executable policy"]);
    let oid = git(&source, &["rev-parse", "HEAD"]);
    git(
        &authority.git_dir(),
        &["fetch", source.to_str().unwrap(), &oid],
    );
    git(
        &authority.git_dir(),
        &["update-ref", "refs/heads/trunk", &oid],
    );
    let error = match TrustedPolicy::load(&authority, &oid) {
        Ok(_) => panic!("accepted symlink Git mode"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("regular-file mode"), "{error:#}");
}

#[test]
fn candidate_bootstrap_inputs_require_regular_git_modes() {
    let (_temp, authority, source) = fixture();
    let blob = git(&source, &["hash-object", "input"]);
    git(
        &source,
        &[
            "update-index",
            "--cacheinfo",
            &format!("120000,{blob},input"),
        ],
    );
    git(&source, &["commit", "-m", "symlink-mode candidate input"]);
    let oid = git(&source, &["rev-parse", "HEAD"]);
    git(
        &authority.git_dir(),
        &["fetch", source.to_str().unwrap(), &oid],
    );
    let policy = TrustedPolicy::load(&authority, &oid).unwrap();
    assert!(source.join("input").is_file());
    assert!(
        policy
            .check_candidate_input("input")
            .unwrap_err()
            .to_string()
            .contains("regular-file mode")
    );
}
