use mai::core::{Repository, overlap, validate_path};
use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn setup() -> (tempfile::TempDir, Repository) {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repository::init(tmp.path().join("project"), None).unwrap();
    (tmp, r)
}
fn write(r: &Repository, a: &str, p: &str, s: &str) {
    let p = r.workspace_path(a).unwrap().join(p);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}
fn git(path: &Path, args: &[&str]) -> Vec<u8> {
    let o = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    o.stdout
}
#[test]
fn non_overlapping_stale_agents_commit_as_consecutive_revisions() {
    let (_t, r) = setup();
    for a in ["alpha", "beta"] {
        r.create_workspace(a).unwrap();
    }
    write(&r, "alpha", "src/first.py", "alpha = 1\n");
    write(&r, "beta", "test/second.py", "beta = 2\n");
    assert_eq!(r.submit("alpha", "first", None).unwrap().revision, 1);
    let two = r.submit("beta", "second", None).unwrap();
    assert_eq!(two.revision, 2);
    assert!(two.rebased);
    assert_eq!(r.show("r1", "src/first.py").unwrap(), b"alpha = 1\n");
    assert_eq!(r.show("r2", "src/first.py").unwrap(), b"alpha = 1\n");
    assert_eq!(r.show("r2", "test/second.py").unwrap(), b"beta = 2\n");
    assert_eq!(r.status().unwrap()["workspaces"][1]["dirty"], false);
    let h = r.history(10).unwrap();
    assert_eq!(h[0]["agent"], "beta");
    assert_eq!(
        h.as_array()
            .unwrap()
            .iter()
            .map(|v| v["revision"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
}
#[test]
fn same_path_stale_submission_fails_without_overwriting() {
    let (_t, r) = setup();
    for a in ["alpha", "beta"] {
        r.create_workspace(a).unwrap();
        write(&r, a, "README.md", a);
    }
    r.submit("alpha", "first", None).unwrap();
    assert!(
        r.submit("beta", "lose", None)
            .unwrap_err()
            .to_string()
            .contains("overlapping")
    );
    assert_eq!(r.revision().unwrap(), 1);
    assert_eq!(r.show("HEAD", "README.md").unwrap(), b"alpha");
    assert_eq!(
        fs::read(r.workspace_path("beta").unwrap().join("README.md")).unwrap(),
        b"beta"
    );
}
#[test]
fn foreign_lease_blocks_submission_then_release_allows_it() {
    let (_t, r) = setup();
    for a in ["alpha", "beta"] {
        r.create_workspace(a).unwrap();
    }
    r.lease("alpha", &["src/".into()], 30).unwrap();
    assert!(r.lease("beta", &["src/foo.py".into()], 30).is_err());
    write(&r, "beta", "src/foo.py", "data");
    assert!(
        r.submit("beta", "blocked", None)
            .unwrap_err()
            .to_string()
            .contains("reserved")
    );
    assert_eq!(r.revision().unwrap(), 0);
    r.release("alpha", &[]).unwrap();
    assert_eq!(r.submit("beta", "accepted", None).unwrap().revision, 1);
}
#[test]
fn path_prefixes_are_components_not_text_prefixes() {
    let (_t, r) = setup();
    for a in ["alpha", "beta"] {
        r.create_workspace(a).unwrap();
    }
    r.lease("alpha", &["src".into()], 30).unwrap();
    r.lease("beta", &["src-utils".into()], 30).unwrap();
    write(&r, "beta", "src-utils/file", "data");
    r.submit("beta", "good", None).unwrap();
    assert!(!overlap("src", "src-utils"));
    assert!(overlap("src", "src/file"));
}
#[test]
fn commit_deletion_and_executable_flag() {
    use std::os::unix::fs::PermissionsExt;
    let (_t, r) = setup();
    r.create_workspace("alpha").unwrap();
    write(&r, "alpha", "script.sh", "#!/bin/sh\n");
    let p = r.workspace_path("alpha").unwrap().join("script.sh");
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    r.submit("alpha", "script", None).unwrap();
    assert!(
        r.git
            .text(&["ls-tree", "HEAD", "script.sh"])
            .unwrap()
            .contains("100755")
    );
    fs::remove_file(p).unwrap();
    r.submit("alpha", "remove", None).unwrap();
    assert_eq!(r.revision().unwrap(), 2);
    assert!(
        r.git
            .run(&["ls-tree", "HEAD", "script.sh"])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn check_gate_rejects_and_preserves_changes() {
    let (_t, r) = setup();
    r.create_workspace("alpha").unwrap();
    write(&r, "alpha", "file", "data");
    assert!(
        r.submit("alpha", "bad", Some("exit 4"))
            .unwrap_err()
            .to_string()
            .contains("Verification command failed")
    );
    assert_eq!(r.revision().unwrap(), 0);
    assert_eq!(
        r.submit("alpha", "pass", Some("test -f file"))
            .unwrap()
            .revision,
        1
    );
}
#[test]
fn refresh_requires_clean_workspace_or_explicit_discard() {
    let (_t, r) = setup();
    for a in ["alpha", "beta"] {
        r.create_workspace(a).unwrap();
    }
    write(&r, "alpha", "file-a", "committed");
    r.submit("alpha", "add", None).unwrap();
    write(&r, "beta", "draft.txt", "draft");
    assert!(r.refresh("beta", false).is_err());
    assert_eq!(
        fs::read(r.workspace_path("beta").unwrap().join("draft.txt")).unwrap(),
        b"draft"
    );
    assert_eq!(r.refresh("beta", true).unwrap()["discarded"], true);
    assert!(!r.workspace_path("beta").unwrap().join("draft.txt").exists());
    assert_eq!(
        fs::read(r.workspace_path("beta").unwrap().join("file-a")).unwrap(),
        b"committed"
    );
}
fn concurrent(agents: usize, hotspot: bool) {
    let (_t, r) = setup();
    for i in 0..agents {
        let a = format!("a{i}");
        r.create_workspace(&a).unwrap();
        write(
            &r,
            &a,
            &if hotspot {
                "shared".into()
            } else {
                format!("parts/{a}.txt")
            },
            &a,
        );
    }
    let barrier = std::sync::Barrier::new(agents);
    let results = std::thread::scope(|scope| {
        let handles = (0..agents)
            .map(|i| {
                let r = &r;
                let b = &barrier;
                scope.spawn(move || {
                    b.wait();
                    r.submit(&format!("a{i}"), "parallel", None)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut revisions = results
        .iter()
        .filter_map(|s| s.as_ref().ok().map(|s| s.revision))
        .collect::<Vec<_>>();
    revisions.sort();
    let accepted = if hotspot { 1 } else { agents };
    assert_eq!(revisions, (1..=accepted as u64).collect::<Vec<_>>());
    assert_eq!(r.revision().unwrap(), accepted as u64);
    for e in results.iter().filter_map(|s| s.as_ref().err()) {
        assert!(e.to_string().contains("overlapping"));
    }
    if !hotspot {
        for i in 0..agents {
            let a = format!("a{i}");
            assert_eq!(
                r.show("HEAD", &format!("parts/{a}.txt")).unwrap(),
                a.as_bytes()
            );
        }
    }
}
#[test]
fn parallel_submissions_are_serialized_and_both_survive() {
    concurrent(2, false);
}
#[test]
fn eight_parallel_disjoint_agents_preserve_all_changes() {
    concurrent(8, false);
}
#[test]
fn concurrent_contended_agents_have_exactly_one_winner() {
    concurrent(8, true);
}
#[test]
fn directory_file_replacement() {
    let (_t, r) = setup();
    r.create_workspace("alpha").unwrap();
    write(&r, "alpha", "src/old.txt", "old");
    r.submit("alpha", "directory", None).unwrap();
    let p = r.workspace_path("alpha").unwrap();
    fs::remove_file(p.join("src/old.txt")).unwrap();
    fs::remove_dir(p.join("src")).unwrap();
    write(&r, "alpha", "src", "file");
    r.submit("alpha", "replace", None).unwrap();
    assert_eq!(r.show("HEAD", "src").unwrap(), b"file");
    fs::remove_file(p.join("src")).unwrap();
    write(&r, "alpha", "src/new", "new");
    r.submit("alpha", "reverse", None).unwrap();
    assert_eq!(r.show("HEAD", "src/new").unwrap(), b"new");
}
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn json_check_output_is_not_mixed_with_response() {
    let (_t, r) = setup();
    r.create_workspace("alpha").unwrap();
    write(&r, "alpha", "file", "data");
    let o = cli(&[
        "--json",
        "submit",
        r.root.to_str().unwrap(),
        "alpha",
        "-m",
        "message",
        "--check",
        "echo hello-from-check",
    ]);
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["result"]["revision"], 1);
}
#[test]
fn json_cli_and_version_flag() {
    let (_t, r) = setup();
    let o = cli(&["--json", "status", r.root.to_str().unwrap()]);
    assert!(o.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap()["ok"],
        true
    );
    let o = cli(&["--version"]);
    assert_eq!(String::from_utf8(o.stdout).unwrap().trim(), "mai 0.0.1");
    for args in [
        vec!["--json", "submit"],
        vec!["--json", "status", "/does-not-exist"],
    ] {
        let o = cli(&args);
        assert_eq!(o.status.code(), Some(2));
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stdout).unwrap()["ok"],
            false
        );
    }
}
#[test]
fn git_remote_mirror_fast_forward() {
    let (t, r) = setup();
    let remote = t.path().join("downstream.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap()
            .success()
    );
    r.create_workspace("alpha").unwrap();
    write(&r, "alpha", "hello", "hello");
    r.submit("alpha", "greet", None).unwrap();
    r.link_github(remote.to_str().unwrap()).unwrap();
    r.sync_github().unwrap();
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(remote)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        r.head().unwrap()
    );
}
#[test]
fn import_existing_git_head_at_r0() {
    let t = tempfile::tempdir().unwrap();
    let origin = t.path().join("origin");
    fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q"]);
    fs::write(origin.join("doc.txt"), "original").unwrap();
    git(&origin, &["add", "-A"]);
    git(
        &origin,
        &[
            "-c",
            "user.name=Tester",
            "-c",
            "user.email=t@test.local",
            "commit",
            "-qm",
            "origin",
        ],
    );
    let r = Repository::init(t.path().join("imported"), origin.to_str()).unwrap();
    assert_eq!(r.revision().unwrap(), 0);
    assert_eq!(r.show("r0", "doc.txt").unwrap(), b"original");
    r.create_workspace("a").unwrap();
    write(&r, "a", "doc.txt", "changed");
    assert_eq!(r.submit("a", "change", None).unwrap().revision, 1);
}
#[test]
fn python_format_and_legacy_prototype_are_readable() {
    for legacy in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let control = t.path().join(if legacy { ".agenttrunk" } else { ".mai" });
        fs::create_dir(&control).unwrap();
        fs::create_dir(t.path().join("workspaces")).unwrap();
        let bare = control.join("repo.git");
        assert!(
            Command::new("git")
                .args(["init", "--bare", "--quiet", "--initial-branch=main"])
                .arg(&bare)
                .status()
                .unwrap()
                .success()
        );
        let bundle =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python-v0.1.1.bundle");
        let mut c = Command::new("git");
        let o = c
            .arg("--git-dir")
            .arg(&bare)
            .args(["fetch", "--quiet"])
            .arg(bundle)
            .arg("refs/*:refs/*")
            .output()
            .unwrap();
        assert!(o.status.success());
        fs::write(
            control.join("metadata.sqlite3"),
            include_bytes!("fixtures/python-v0.1.1.sqlite3"),
        )
        .unwrap();
        let r = Repository::open(t.path()).unwrap();
        if legacy {
            let base = r.git.text(&["rev-parse", "refs/mai/base"]).unwrap();
            r.git
                .run(&["update-ref", "refs/agenttrunk/base", &base])
                .unwrap();
            r.git.run(&["update-ref", "-d", "refs/mai/base"]).unwrap();
        }
        assert_eq!(r.revision().unwrap(), 1);
        assert_eq!(r.show("r1", "original.txt").unwrap(), b"Python baseline\n");
        r.create_workspace("rust").unwrap();
        write(&r, "rust", "new.txt", "Rust\n");
        assert_eq!(r.submit("rust", "compat", None).unwrap().revision, 2);
        assert_eq!(r.history(10).unwrap()[1]["agent"], "fixture");
    }
}
#[test]
fn validation_and_expired_exact_release() {
    let (_t, r) = setup();
    assert!(r.create_workspace("../escape").is_err());
    for p in ["", "src//", "../x", "/x", "src/../x", "src\\x"] {
        assert!(validate_path(p).is_err());
    }
    r.create_workspace("a").unwrap();
    assert!(r.lease("a", &["x".into()], 0).is_err());
    assert!(r.lease("a", &["x".into()], 86401).is_err());
    r.lease("a", &["src".into(), "src/a".into()], 30).unwrap();
    assert_eq!(r.release("a", &["src".into()]).unwrap()["released"], 1);
    assert_eq!(r.leases().unwrap().as_array().unwrap().len(), 1);
    let db = rusqlite::Connection::open(r.control.join("metadata.sqlite3")).unwrap();
    db.execute("UPDATE leases SET expires_at=0", []).unwrap();
    assert!(r.leases().unwrap().as_array().unwrap().is_empty());
    assert!(r.submit("a", "empty", None).is_err());
    assert!(r.submit("a", " ", None).is_err());
    assert!(r.show("r9", "file").is_err());
    assert!(r.history(0).is_err());
}
#[test]
fn binary_rename_symlink_and_path_collision() {
    use std::os::unix::fs::symlink;
    let (_t, r) = setup();
    for a in ["a", "b"] {
        r.create_workspace(a).unwrap();
    }
    let p = r.workspace_path("a").unwrap();
    fs::write(p.join("binary"), [0, 255, 1]).unwrap();
    symlink("/does/not/exist", p.join("link")).unwrap();
    r.submit("a", "binary", None).unwrap();
    assert_eq!(r.show("HEAD", "binary").unwrap(), [0, 255, 1]);
    assert_eq!(r.show("HEAD", "link").unwrap(), b"/does/not/exist");
    fs::rename(p.join("binary"), p.join("renamed")).unwrap();
    r.submit("a", "rename", None).unwrap();
    assert!(r.show("HEAD", "binary").is_err());
    assert_eq!(r.show("HEAD", "renamed").unwrap(), [0, 255, 1]);
    write(&r, "b", "link/file", "collision");
    assert!(
        r.submit("b", "collision", None)
            .unwrap_err()
            .to_string()
            .contains("overlapping")
    );
}
#[test]
fn accepted_receipt_recovers_without_destroying_later_edits() {
    let (_t, r) = setup();
    r.create_workspace("a").unwrap();
    let p = r.workspace_path("a").unwrap();
    let base = r.head().unwrap();
    write(&r, "a", "file", "accepted");
    let accepted = r.submit("a", "accepted", None).unwrap();
    git(&p, &["reset", "--hard", &base]);
    write(&r, "a", "file", "accepted");
    write(&r, "a", "later", "preserve me");
    let recovered = r.submit("a", "retry", None).unwrap();
    assert!(recovered.recovered);
    assert!(!recovered.workspace_updated);
    assert_eq!(recovered.commit, accepted.commit);
    assert_eq!(r.revision().unwrap(), 1);
    assert_eq!(fs::read(p.join("later")).unwrap(), b"preserve me");
}
#[test]
fn same_agent_concurrent_submission_does_not_duplicate() {
    let (_t, r) = setup();
    r.create_workspace("a").unwrap();
    write(&r, "a", "file", "data");
    let results = std::thread::scope(|s| {
        let a = s.spawn(|| r.submit("a", "one", None));
        let b = s.spawn(|| r.submit("a", "two", None));
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(r.revision().unwrap(), 1);
}
#[test]
fn fanout_benchmark_reports_conflicts_and_integrity() {
    let o = Command::new(env!("CARGO_BIN_EXE_mai-bench"))
        .args([
            "--agents",
            "4",
            "--workers",
            "2",
            "--hotspot-agents",
            "3",
            "--cli",
            env!("CARGO_BIN_EXE_mai"),
        ])
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{} {}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(
        v["implementation"],
        concat!("mai Rust ", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(v["integrity_ok"], true);
    assert_eq!(v["accepted"], 2);
    assert_eq!(v["conflicts"], 2);
    assert_eq!(v["final_revision"], 2);
}
#[test]
fn separate_process_disjoint_publications() {
    let (_t, r) = setup();
    let agents = (0..16).map(|i| format!("p{i}")).collect::<Vec<_>>();
    for a in &agents {
        r.create_workspace(a).unwrap();
        write(&r, a, a, a);
    }
    let children = agents
        .iter()
        .map(|a| {
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["--json", "submit"])
                .arg(&r.root)
                .args([a, "-m", a])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut revisions = Vec::new();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        revisions.push(v["result"]["revision"].as_u64().unwrap());
    }
    revisions.sort();
    assert_eq!(revisions, (1..=16).collect::<Vec<_>>());
    for a in agents {
        assert_eq!(r.show("HEAD", &a).unwrap(), a.as_bytes());
    }
}
#[test]
fn recovery_without_receipt_and_unpublished_receipt_is_not_acceptance() {
    let (_t, r) = setup();
    r.create_workspace("a").unwrap();
    let base = r.head().unwrap();
    write(&r, "a", "file", "accepted");
    let accepted = r.submit("a", "accepted", None).unwrap();
    let p = r.workspace_path("a").unwrap();
    git(&p, &["reset", "--hard", &base]);
    write(&r, "a", "draft", "keep");
    r.git
        .run(&["update-ref", "-d", "refs/mai/receipts/a"])
        .unwrap();
    let recovered = r.submit("a", "retry", None).unwrap();
    assert!(recovered.recovered);
    assert_eq!(recovered.commit, accepted.commit);
    r.git
        .run(&["update-ref", "refs/mai/receipts/a", &accepted.commit])
        .unwrap();
    r.git
        .run(&["update-ref", "refs/heads/main", &base, &accepted.commit])
        .unwrap();
    let new = r.submit("a", "actually publish", None).unwrap();
    assert!(!new.recovered);
    assert_eq!(new.revision, 1);
    assert_eq!(r.show("HEAD", "draft").unwrap(), b"keep");
}
#[test]
fn cas_rejects_external_writer_and_preserves_receipt() {
    let (_t, r) = setup();
    r.create_workspace("a").unwrap();
    let old = r.head().unwrap();
    write(&r, "a", "file", "accepted");
    let accepted = r.submit("a", "accepted", None).unwrap();
    assert!(
        r.git
            .publish(
                &old,
                &accepted.commit,
                "refs/mai/receipts/b",
                &"0".repeat(40)
            )
            .is_err()
    );
    assert_eq!(r.head().unwrap(), accepted.commit);
    assert!(
        r.git
            .run(&["rev-parse", "--verify", "refs/mai/receipts/b"])
            .is_err()
    );
}
#[test]
fn lease_acquired_during_preparation_is_checked_at_publication() {
    let (_t, r) = setup();
    for a in ["a", "b"] {
        r.create_workspace(a).unwrap();
    }
    write(&r, "b", "file", "draft");
    let check = format!(
        "'{}' lease '{}' a file",
        env!("CARGO_BIN_EXE_mai"),
        r.root.display()
    );
    assert!(
        r.submit("b", "blocked", Some(&check))
            .unwrap_err()
            .to_string()
            .contains("reserved")
    );
    assert_eq!(r.revision().unwrap(), 0);
    r.release("a", &[]).unwrap();
    r.submit("b", "accepted", None).unwrap();
}
#[test]
fn git_hooks_are_not_executed() {
    let (_t, r) = setup();
    let hook = r.git.bare.join("hooks/post-checkout");
    fs::write(&hook, "#!/bin/sh\nexit 99\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    r.create_workspace("a").unwrap();
    write(&r, "a", "file", "data");
    r.submit("a", "accepted", None).unwrap();
}
#[test]
fn process_termination_after_acceptance_preserves_revision_and_draft() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let (tmp, r) = setup();
    r.create_workspace("a").unwrap();
    write(&r, "a", "file", "accepted");
    let tool_dir = tmp.path().join("tools");
    fs::create_dir(&tool_dir).unwrap();
    let ready = tmp.path().join("ready");
    let done = tmp.path().join("done");
    let real_git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real_git = String::from_utf8(real_git.stdout).unwrap();
    let wrapper = tool_dir.join("git");
    fs::write(&wrapper,format!("#!/bin/sh\ncase \"$*\" in\n  *'reset --hard'*)\n    printf ready > \"$MAI_TEST_READY\"\n    while kill -0 \"$PPID\" 2>/dev/null; do sleep 0.01; done\n    printf done > \"$MAI_TEST_DONE\"\n    exit 1;;\nesac\nexec '{}' \"$@\"\n",real_git.trim())).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(["--json", "submit"])
        .arg(&r.root)
        .args(["a", "-m", "accepted"])
        .env(
            "PATH",
            format!("{}:{}", tool_dir.display(), std::env::var("PATH").unwrap()),
        )
        .env("MAI_TEST_READY", &ready)
        .env("MAI_TEST_DONE", &done)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !ready.exists() && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let was_ready = ready.exists();
    let _ = child.kill();
    child.wait().unwrap();
    assert!(was_ready, "CLI did not reach post-publication reset");
    let start = Instant::now();
    while !done.exists() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(done.exists());
    assert_eq!(r.revision().unwrap(), 1);
    write(&r, "a", "later", "preserve");
    let recovered = r.submit("a", "retry", None).unwrap();
    assert!(recovered.recovered);
    assert!(!recovered.workspace_updated);
    assert_eq!(r.revision().unwrap(), 1);
    assert_eq!(r.show("r1", "file").unwrap(), b"accepted");
    assert_eq!(
        fs::read(r.workspace_path("a").unwrap().join("later")).unwrap(),
        b"preserve"
    );
}
#[test]
fn benchmark_emits_json_when_submissions_are_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("fake-mai");
    fs::write(&fake,"#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'mai test'; exit 0; fi\nprintf '%s\\n' '{\"ok\":false,\"error\":\"Publication contention exceeded 64 attempts\"}'\nexit 2\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_mai-bench"))
        .args(["--agents", "2", "--workers", "2", "--cli"])
        .arg(fake)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["integrity_ok"], false);
    assert_eq!(v["accepted"], 0);
    assert_eq!(v["other_errors"].as_array().unwrap().len(), 2);
}
#[test]
fn divergent_remote_is_never_force_pushed() {
    let (tmp, r) = setup();
    let remote = tmp.path().join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap()
            .success()
    );
    r.create_workspace("a").unwrap();
    write(&r, "a", "file", "initial");
    r.submit("a", "initial", None).unwrap();
    r.link_github(remote.to_str().unwrap()).unwrap();
    r.sync_github().unwrap();
    let head = r.head().unwrap();
    let tree = r
        .git
        .text(&["rev-parse", &format!("{head}^{{tree}}")])
        .unwrap();
    let o = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["commit-tree", &tree, "-p", &head, "-m", "remote change"])
        .env("GIT_AUTHOR_NAME", "Remote")
        .env("GIT_AUTHOR_EMAIL", "r@mai.invalid")
        .env("GIT_COMMITTER_NAME", "Remote")
        .env("GIT_COMMITTER_EMAIL", "r@mai.invalid")
        .output()
        .unwrap();
    assert!(o.status.success());
    let remote_head = String::from_utf8(o.stdout).unwrap();
    let remote_head = remote_head.trim();
    assert!(
        Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["update-ref", "refs/heads/main", remote_head])
            .status()
            .unwrap()
            .success()
    );
    write(&r, "a", "file", "local");
    r.submit("a", "local change", None).unwrap();
    assert!(r.sync_github().is_err());
    let o = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(o.stdout).unwrap().trim(), remote_head);
    assert_eq!(r.revision().unwrap(), 2);
}
#[test]
fn non_utf8_names_reject_before_publication_and_preserve_files() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let (_t, r) = setup();
    r.create_workspace("a").unwrap();
    let path = r
        .workspace_path("a")
        .unwrap()
        .join(OsString::from_vec(vec![0xff]));
    fs::write(&path, b"draft").unwrap();
    assert!(r.submit("a", "unsupported", None).is_err());
    assert_eq!(r.revision().unwrap(), 0);
    assert_eq!(fs::read(path).unwrap(), b"draft");
}
#[test]
fn workspace_symlink_and_foreign_git_directory_are_rejected() {
    use std::os::unix::fs::symlink;
    let (tmp, r) = setup();
    r.create_workspace("a").unwrap();
    let path = r.workspace_path("a").unwrap();
    let moved = tmp.path().join("moved");
    fs::rename(&path, &moved).unwrap();
    symlink(&moved, &path).unwrap();
    assert!(r.submit("a", "escape", None).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    git(&path, &["init", "-q"]);
    assert!(r.refresh("a", false).is_err());
    assert_eq!(r.revision().unwrap(), 0);
}
#[test]
fn symlinked_workspace_registry_rejects_creation() {
    use std::os::unix::fs::symlink;
    let (tmp, r) = setup();
    let registry = r.root.join("workspaces");
    let moved = tmp.path().join("moved");
    fs::rename(&registry, &moved).unwrap();
    symlink(&moved, &registry).unwrap();
    assert!(r.create_workspace("a").is_err());
    assert!(!moved.join("a").exists());
    assert_eq!(r.revision().unwrap(), 0);
}
