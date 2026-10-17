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
    let entries: String = files
        .iter()
        .map(|(name, content)| {
            let blob = git(a, &["hash-object", "-w", "--stdin"], content);
            format!("100644 blob {blob}\t{name}\n")
        })
        .collect();
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
