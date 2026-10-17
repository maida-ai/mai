//! Independent refusal and full-source-preservation cases for copy continuation.
#[path = "continuation_support/mod.rs"]
mod support;
use std::fs;
use support::*;

fn refused(f: &Fixture) {
    let before = snapshot(&f.source);
    let output = f.continue_to(&f.destination);
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !f.destination.exists(),
        "preflight refusal must not reserve a destination"
    );
    assert_eq!(snapshot(&f.source), before);
    f.assert_authority_unchanged();
}

#[test]
fn tracked_dirty_and_nonignored_untracked_sources_are_preserved_and_refused() {
    let f = Fixture::new();
    fs::write(f.source.join("local.txt"), "precious unstaged work\n").unwrap();
    refused(&f);
    git(&f.source, &["add", "local.txt"]);
    refused(&f);
    git(&f.source, &["reset", "--hard", "HEAD"]);
    fs::write(f.source.join("untracked.txt"), "precious untracked work\n").unwrap();
    refused(&f);
}

#[test]
fn assume_unchanged_and_skip_worktree_cannot_hide_source_dirt() {
    let f = Fixture::new();
    git(
        &f.source,
        &["update-index", "--assume-unchanged", "local.txt"],
    );
    fs::write(f.source.join("local.txt"), "hidden dirty work\n").unwrap();
    refused(&f);
    git(
        &f.source,
        &["update-index", "--no-assume-unchanged", "local.txt"],
    );
    git(&f.source, &["reset", "--hard", "HEAD"]);
    git(&f.source, &["update-index", "--skip-worktree", "local.txt"]);
    fs::write(f.source.join("local.txt"), "hidden skip-worktree work\n").unwrap();
    refused(&f);
}

#[test]
fn wrong_generation_wrong_agent_and_missing_submitted_ancestor_refuse() {
    let f = Fixture::new();
    let before = snapshot(&f.source);
    assert!(
        !f.continue_generation(&f.destination, &f.change, "2")
            .status
            .success()
    );
    assert_eq!(snapshot(&f.source), before);
    assert!(!f.destination.exists());
    git(&f.source, &["config", "mai.agent", "bob"]);
    refused(&f);
    git(&f.source, &["config", "mai.agent", "alice"]);
    git(&f.source, &["reset", "--hard", &f.baseline]);
    refused(&f);
}

#[test]
fn merged_local_suffix_is_ambiguous_and_refused() {
    let f = Fixture::new();
    let branch = git(&f.source, &["symbolic-ref", "--short", "HEAD"]);
    git(&f.source, &["checkout", "-b", "side", &f.submitted]);
    f.commit_local("side.txt", "side branch work\n", "side suffix");
    git(&f.source, &["checkout", &branch]);
    git(
        &f.source,
        &["merge", "--no-ff", "-m", "ambiguous merge", "side"],
    );
    refused(&f);
}

#[test]
fn existing_destination_and_source_nested_destination_are_never_adopted() {
    let f = Fixture::new();
    fs::create_dir(&f.destination).unwrap();
    fs::write(f.destination.join("precious"), "existing destination\n").unwrap();
    let before = snapshot(&f.source);
    let existing = snapshot(&f.destination);
    assert!(!f.continue_to(&f.destination).status.success());
    assert_eq!(snapshot(&f.destination), existing);
    assert_eq!(snapshot(&f.source), before);
    let nested = f.source.join("ignored/continued");
    assert!(!f.continue_to(&nested).status.success());
    assert!(!nested.exists());
    assert_eq!(snapshot(&f.source), before);
    #[cfg(unix)]
    {
        let alias = f.temp.path().join("destination-alias");
        std::os::unix::fs::symlink(&f.destination, &alias).unwrap();
        assert!(!f.continue_to(&alias).status.success());
        assert_eq!(snapshot(&f.destination), existing);
        assert_eq!(snapshot(&f.source), before);
    }
}

#[test]
fn pending_registered_suffix_cannot_be_replayed_as_unsubmitted_work() {
    let f = Fixture::new();
    let mut engine = f.start();
    git(
        &f.source,
        &["push", "-o", "agent=alice", "origin", "HEAD:refs/for/main"],
    );
    f.stop(&mut engine);
    refused(&f);
}

#[test]
fn rejected_registered_suffix_cannot_be_replayed_as_unsubmitted_work() {
    let f = Fixture::new();
    git(&f.source, &["reset", "--soft", &f.submitted]);
    fs::write(f.source.join("fail"), "rejected suffix\n").unwrap();
    git(&f.source, &["add", "fail"]);
    git(&f.source, &["commit", "-m", "registered rejected suffix"]);
    let commit = git(&f.source, &["rev-parse", "HEAD"]);
    let change = format!("alice-{commit}");
    let mut engine = f.start();
    git(
        &f.source,
        &["push", "-o", "agent=alice", "origin", "HEAD:refs/for/main"],
    );
    let output = cli(&[
        "integrate".as_ref(),
        f.project.as_os_str(),
        change.as_ref(),
        "--mode".as_ref(),
        "textual".as_ref(),
        "--wait-seconds".as_ref(),
        "30".as_ref(),
    ]);
    assert!(!output.status.success());
    f.stop(&mut engine);
    refused(&f);
}

#[test]
fn already_landed_registered_suffix_cannot_be_duplicated() {
    let mut f = Fixture::new();
    let mut engine = f.start();
    git(
        &f.source,
        &["push", "-o", "agent=alice", "origin", "HEAD:refs/for/main"],
    );
    f.trunk = f.integrate(&format!("alice-{}", f.suffix))["commit"]
        .as_str()
        .unwrap()
        .into();
    f.stop(&mut engine);
    refused(&f);
}

#[cfg(unix)]
#[test]
fn source_clean_filters_are_refused_before_any_status_can_execute_them() {
    use std::time::{Duration, SystemTime};
    let f = Fixture::new();
    let executed = f.temp.path().join("source-filter-executed");
    let command = format!(
        "printf triggered >> '{}'; cat",
        executed.to_str().unwrap().replace('\'', "'\\''")
    );
    git(&f.source, &["config", "filter.tripwire.clean", &command]);
    git(&f.source, &["config", "filter.tripwire.required", "true"]);
    fs::write(
        f.source.join(".git/info/attributes"),
        "*.txt filter=tripwire\n",
    )
    .unwrap();
    // Force a content comparison while retaining the committed bytes.
    fs::File::open(f.source.join("local.txt"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(2)))
        .unwrap();
    let before = snapshot(&f.source);
    let output = f.continue_to(&f.destination);
    assert!(!output.status.success());
    assert!(
        !executed.exists(),
        "source inspection executed a source-controlled clean filter"
    );
    assert_eq!(snapshot(&f.source), before);
    assert!(!f.destination.exists());
}

#[test]
fn ongoing_source_git_operation_is_not_a_clean_continuation() {
    let f = Fixture::new();
    fs::write(
        f.source.join(".git/MERGE_HEAD"),
        format!("{}\n", f.submitted),
    )
    .unwrap();
    refused(&f);
    fs::remove_file(f.source.join(".git/MERGE_HEAD")).unwrap();
    fs::create_dir(f.source.join(".git/rebase-merge")).unwrap();
    fs::write(
        f.source.join(".git/rebase-merge/head-name"),
        "refs/heads/original\n",
    )
    .unwrap();
    refused(&f);
}

#[test]
fn linked_source_worktree_is_preserved_and_refused_explicitly() {
    let mut f = Fixture::new();
    let linked = f.temp.path().join("linked-source");
    git(
        &f.source,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    let original = std::mem::replace(&mut f.source, linked);
    let original_before = snapshot(&original);
    refused(&f);
    assert_eq!(snapshot(&original), original_before);
}

#[test]
fn grafted_history_cannot_invent_the_selected_submitted_ancestor() {
    let f = Fixture::new();
    git(&f.source, &["checkout", "--detach", &f.baseline]);
    f.commit_local(
        "local.txt",
        "unrelated linear work\n",
        "unrelated source history",
    );
    let head = git(&f.source, &["rev-parse", "HEAD"]);
    fs::write(
        f.source.join(".git/info/grafts"),
        format!("{head} {}\n", f.submitted),
    )
    .unwrap();
    refused(&f);
}

#[cfg(unix)]
#[test]
fn symlinked_source_object_storage_is_not_an_independent_normal_clone() {
    let f = Fixture::new();
    let objects = f.temp.path().join("retained-source-objects");
    fs::rename(f.source.join(".git/objects"), &objects).unwrap();
    std::os::unix::fs::symlink(&objects, f.source.join(".git/objects")).unwrap();
    let objects_before = snapshot(&objects);
    refused(&f);
    assert_eq!(snapshot(&objects), objects_before);
}
