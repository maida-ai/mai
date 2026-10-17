use mai::{
    authority::Authority,
    landing::{MergeMode, prepare_candidate},
    lifecycle::{Lifecycle, Outcome},
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn git(a: &Authority, args: &[&str], input: &str) -> String {
    let mut child = Command::new("git")
        .arg("--git-dir")
        .arg(a.git_dir())
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn commit(a: &Authority, parent: &str, files: &[(&str, &str)]) -> String {
    let mut entries = String::new();
    for (name, content) in files {
        let blob = git(a, &["hash-object", "-w", "--stdin"], content);
        use std::fmt::Write as _;
        writeln!(entries, "100644 blob {blob}\t{name}").unwrap();
    }
    let tree = git(a, &["mktree"], &entries);
    git(
        a,
        &["commit-tree", &tree, "-p", parent, "-m", "fixture"],
        "",
    )
}
#[test]
fn independent_delta_uses_explicit_parent_and_never_moves_trunk() {
    let tmp = tempfile::tempdir().unwrap();
    let a = Authority::init(&tmp.path().join("a"), None).unwrap();
    let base = git(&a, &["rev-parse", "refs/heads/trunk"], "");
    let submitted = commit(&a, &base, &[("a", "agent")]);
    let trunk = commit(&a, &base, &[("b", "concurrent")]);
    git(&a, &["update-ref", "refs/heads/trunk", &trunk], "");
    let mut life = Lifecycle::new();
    let id = life
        .register("change", "agent", &submitted, vec![])
        .unwrap();
    life.queue(&id).unwrap();
    let prepared = prepare_candidate(&a, &life, &id, MergeMode::Textual).unwrap();
    assert_eq!(prepared.base, base);
    assert_eq!(prepared.trunk, trunk);
    assert_eq!(
        git(&a, &["ls-tree", "--name-only", &prepared.tree], ""),
        "a\nb"
    );
    assert_eq!(git(&a, &["rev-parse", "refs/heads/trunk"], ""), trunk);
}
#[test]
fn dependency_delta_does_not_replay_squashed_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let a = Authority::init(&tmp.path().join("a"), None).unwrap();
    let base = git(&a, &["rev-parse", "refs/heads/trunk"], "");
    let parent = commit(&a, &base, &[("a", "parent")]);
    let child = commit(&a, &parent, &[("a", "parent"), ("b", "child")]);
    let squash = commit(&a, &base, &[("a", "parent"), ("c", "unrelated")]);
    let trunk = commit(&a, &squash, &[("a", "later edit"), ("c", "unrelated")]);
    git(&a, &["update-ref", "refs/heads/trunk", &trunk], "");
    let mut life = Lifecycle::new();
    let p = life.register("parent", "agent", &parent, vec![]).unwrap();
    life.queue(&p).unwrap();
    let id = life
        .register("child", "agent", &child, vec![p.clone()])
        .unwrap();
    life.queue(&id).unwrap();
    assert!(prepare_candidate(&a, &life, &id, MergeMode::Textual).is_err());
    let attempt = life.begin_attempt(&p).unwrap();
    life.complete_attempt(
        &p,
        attempt,
        Outcome::Landed {
            revision: 1,
            commit: squash,
        },
    )
    .unwrap();
    let prepared = prepare_candidate(&a, &life, &id, MergeMode::Textual).unwrap();
    assert_eq!(prepared.base, parent);
    assert_eq!(
        git(&a, &["show", &format!("{}:a", prepared.tree)], ""),
        "later edit"
    );
    assert_eq!(
        git(&a, &["show", &format!("{}:b", prepared.tree)], ""),
        "child"
    );
}
#[test]
fn conflicts_and_unregistered_parent_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let a = Authority::init(&tmp.path().join("a"), None).unwrap();
    let base = git(&a, &["rev-parse", "refs/heads/trunk"], "");
    let submitted = commit(&a, &base, &[("a", "agent")]);
    let trunk = commit(&a, &base, &[("a", "concurrent")]);
    git(&a, &["update-ref", "refs/heads/trunk", &trunk], "");
    let mut life = Lifecycle::new();
    let id = life
        .register("change", "agent", &submitted, vec![])
        .unwrap();
    life.queue(&id).unwrap();
    assert!(prepare_candidate(&a, &life, &id, MergeMode::Textual).is_err());
    let child = commit(&a, &submitted, &[("b", "child")]);
    let id = life.register("child", "agent", &child, vec![]).unwrap();
    life.queue(&id).unwrap();
    assert!(prepare_candidate(&a, &life, &id, MergeMode::Textual).is_err());
    assert_eq!(git(&a, &["rev-parse", "refs/heads/trunk"], ""), trunk);
}

#[cfg(unix)]
fn publication_fixture() -> (tempfile::TempDir, Authority, mai::lease::LeaseStore, String) {
    publication_fixture_with_gate("test ! -f \"$MAI_CANDIDATE_ROOT/fail\"\n")
}
#[cfg(unix)]
fn publication_fixture_with_gate(
    gate: &str,
) -> (tempfile::TempDir, Authority, mai::lease::LeaseStore, String) {
    let tmp = tempfile::tempdir().unwrap();
    let seed = Authority::init(&tmp.path().join("seed"), None).unwrap();
    let base = git(&seed, &["rev-parse", "trunk"], "");
    let baseline = commit(&seed, &base, &[("gate.sh", gate), ("mai.toml", POLICY)]);
    git(&seed, &["update-ref", "refs/heads/trunk", &baseline], "");
    let a = Authority::init(&tmp.path().join("project"), Some(&seed.git_dir())).unwrap();
    let leases = a.lease_store().unwrap();
    (tmp, a, leases, baseline)
}
#[cfg(unix)]
const POLICY: &str = "version = 1\n[check]\nprogram = \"sh\"\nscript = \"gate.sh\"\ntimeout_ms = 5000\noutput_tail_bytes = 1024\n[validators]\nslots = 1\nport_base = 32000\nports_per_slot = 10\n";
#[cfg(unix)]
fn register(
    a: &Authority,
    name: &str,
    commit: &str,
    dependencies: Vec<mai::lifecycle::GenerationId>,
) -> mai::lifecycle::GenerationId {
    let mut store = mai::state::StateStore::open(a.state_path()).unwrap();
    let mut life = store.load_lifecycle().unwrap();
    let id = life.register(name, "agent", commit, dependencies).unwrap();
    git(
        a,
        &[
            "update-ref",
            &format!("refs/mai/changes/{}/{}", id.change_id, id.generation),
            commit,
        ],
        "",
    );
    life.queue(&id).unwrap();
    store.save_lifecycle(&mut life).unwrap();
    id
}
#[cfg(unix)]
fn submission(a: &Authority, base: &str, extra: &[(&str, &str)]) -> String {
    let mut files = vec![
        ("gate.sh", "test ! -f \"$MAI_CANDIDATE_ROOT/fail\"\n"),
        ("mai.toml", POLICY),
    ];
    files.extend_from_slice(extra);
    commit(a, base, &files)
}
#[cfg(unix)]
#[test]
fn gate_evidence_exact_candidate_lands_once_and_dependency_lands_afterward() {
    use mai::landing::{RunResult, run_one};
    let (_tmp, mut a, mut leases, base) = publication_fixture();
    let first = submission(&a, &base, &[("a", "parent")]);
    let second = submission(&a, &first, &[("a", "parent"), ("b", "child")]);
    let p = register(&a, "parent", &first, vec![]);
    let c = register(&a, "child", &second, vec![p.clone()]);
    assert!(run_one(&mut a, &c, MergeMode::Textual, &mut leases, &[]).is_err());
    let RunResult::Landed {
        revision: 1,
        commit: accepted,
    } = run_one(&mut a, &p, MergeMode::Textual, &mut leases, &[]).unwrap()
    else {
        panic!("not landed")
    };
    assert_ne!(accepted, first);
    assert!(matches!(
        run_one(&mut a, &p, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Landed { revision: 1, .. }
    ));
    assert!(matches!(
        run_one(&mut a, &c, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Landed { revision: 2, .. }
    ));
    let store = mai::state::StateStore::open(a.state_path()).unwrap();
    let records = store.landings().unwrap();
    assert_eq!(records.len(), 2);
    for record in &records {
        let e = store
            .evidence(&record.generation, record.attempt)
            .unwrap()
            .unwrap();
        assert!(e.passed);
        assert_eq!(e.candidate_commit, record.commit);
        assert_eq!(
            git(&a, &["show", "-s", "--format=%T", &record.commit], ""),
            e.candidate_tree
        );
    }
    a.reconcile().unwrap();
}
#[cfg(unix)]
#[test]
fn failed_gate_and_foreign_lease_never_publish() {
    use mai::landing::{RunResult, run_one};
    let (_tmp, mut a, mut leases, base) = publication_fixture();
    let blocked = submission(&a, &base, &[("a", "blocked")]);
    let id = register(&a, "blocked", &blocked, vec![]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    leases.acquire("file:a", "other", now, 60_000).unwrap();
    assert!(run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).is_err());
    let failed = submission(&a, &base, &[("fail", "yes")]);
    let id = register(&a, "failed", &failed, vec![]);
    assert!(matches!(
        run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Rejected {
            reason: mai::lifecycle::ReasonCode::CheckFailed
        }
    ));
    assert_eq!(git(&a, &["rev-parse", "trunk"], ""), base);
    assert!(
        mai::state::StateStore::open(a.state_path())
            .unwrap()
            .landings()
            .unwrap()
            .is_empty()
    );
}
#[cfg(unix)]
#[test]
fn protected_candidate_waits_for_exact_independent_approval() {
    use mai::landing::{RunResult, run_one};
    let (_tmp, mut a, mut leases, base) = publication_fixture();
    let changed = commit(&a, &base, &[("gate.sh", "exit 0\n"), ("mai.toml", POLICY)]);
    let id = register(&a, "protected", &changed, vec![]);
    let RunResult::AwaitingApproval { candidate, .. } =
        run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).unwrap()
    else {
        panic!("expected approval")
    };
    let store = mai::state::StateStore::open(a.state_path()).unwrap();
    assert!(matches!(
        store.load_lifecycle().unwrap().get(&id).unwrap().state(),
        mai::lifecycle::State::AwaitingApproval
    ));
    mai::engine::approve(a.root(), &candidate, "agent", "self", false).unwrap();
    assert!(run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).is_err());
    mai::engine::approve(
        a.root(),
        &candidate,
        "maintainer",
        "reviewed executable change",
        false,
    )
    .unwrap();
    assert!(matches!(
        run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Landed { revision: 1, .. }
    ));
}

#[cfg(unix)]
#[test]
fn successful_exit_with_modified_tracked_tree_is_rejected() {
    use mai::landing::{RunResult, run_one};
    let gate = "printf altered > \"$MAI_CANDIDATE_ROOT/a\"\n";
    let (_tmp, mut a, mut leases, base) = publication_fixture_with_gate(gate);
    let change = commit(
        &a,
        &base,
        &[("gate.sh", gate), ("mai.toml", POLICY), ("a", "original")],
    );
    let id = register(&a, "change", &change, vec![]);
    assert!(matches!(
        run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Rejected {
            reason: mai::lifecycle::ReasonCode::CheckFailed
        }
    ));
    assert_eq!(git(&a, &["rev-parse", "trunk"], ""), base);
}
#[test]
fn candidate_attributes_and_custom_merge_drivers_are_not_implicitly_enabled() {
    let tmp = tempfile::tempdir().unwrap();
    let a = Authority::init(&tmp.path().join("a"), None).unwrap();
    let base = git(&a, &["rev-parse", "trunk"], "");
    let submitted = commit(
        &a,
        &base,
        &[(".gitattributes", "* merge=union\n"), ("a", "hello")],
    );
    let mut life = Lifecycle::new();
    let id = life.register("attrs", "agent", &submitted, vec![]).unwrap();
    life.queue(&id).unwrap();
    assert!(prepare_candidate(&a, &life, &id, MergeMode::Textual).is_err());
    let submitted = commit(&a, &base, &[("a", "hello")]);
    let id = life.register("plain", "agent", &submitted, vec![]).unwrap();
    life.queue(&id).unwrap();
    git(&a, &["config", "merge.custom.driver", "false"], "");
    assert!(prepare_candidate(&a, &life, &id, MergeMode::Textual).is_err());
}

#[cfg(unix)]
#[test]
fn true_conflict_has_one_terminal_outcome_and_rejects_dependents() {
    use mai::landing::{RunResult, run_one};
    use mai::lifecycle::{ReasonCode, State};
    let (_tmp, mut a, mut leases, base) = publication_fixture();
    let winner = submission(&a, &base, &[("a", "winner")]);
    let loser = submission(&a, &base, &[("a", "loser")]);
    let dependent = submission(&a, &loser, &[("a", "loser"), ("b", "dependent")]);
    let w = register(&a, "winner", &winner, vec![]);
    let l = register(&a, "loser", &loser, vec![]);
    let d = register(&a, "dependent", &dependent, vec![l.clone()]);
    run_one(&mut a, &w, MergeMode::Textual, &mut leases, &[]).unwrap();
    assert!(matches!(
        run_one(&mut a, &l, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Rejected {
            reason: ReasonCode::Conflict
        }
    ));
    assert!(matches!(
        run_one(&mut a, &l, MergeMode::Textual, &mut leases, &[]).unwrap(),
        RunResult::Rejected {
            reason: ReasonCode::Conflict
        }
    ));
    let store = mai::state::StateStore::open(a.state_path()).unwrap();
    let life = store.load_lifecycle().unwrap();
    assert!(matches!(
        life.get(&l).unwrap().state(),
        State::Terminal {
            outcome: Outcome::Rejected {
                reason: ReasonCode::Conflict,
                ..
            }
        }
    ));
    assert!(matches!(
        life.get(&d).unwrap().state(),
        State::Terminal {
            outcome: Outcome::Rejected {
                reason: ReasonCode::DependencyFailed,
                ..
            }
        }
    ));
    assert_eq!(store.landings().unwrap().len(), 1);
}
