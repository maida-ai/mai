use mai::state::StateStore;

#[test]
fn creation_is_exclusive_and_open_never_creates() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    assert!(StateStore::open(&path).is_err());
    assert!(!path.exists());
    let store = StateStore::create(&path).unwrap();
    assert!(
        store
            .load_lifecycle()
            .unwrap()
            .submissions()
            .next()
            .is_none()
    );
    assert!(StateStore::create(&path).is_err());
    drop(store);
    StateStore::open(&path).unwrap();
}

use mai::lifecycle::{Diagnostics, GenerationId, Outcome, ReasonCode, State};
use mai::state::{LandingRecord, LifecycleSnapshot, ValidationEvidence};
use rusqlite::Connection;
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn oid(ch: char) -> String {
    ch.to_string().repeat(40)
}
fn running(store: &mut StateStore) -> (LifecycleSnapshot, GenerationId) {
    let mut model = store.load_lifecycle().unwrap();
    let id = model
        .register("change", "agent", &oid('a'), vec![])
        .unwrap();
    model.queue(&id).unwrap();
    assert_eq!(model.begin_attempt(&id).unwrap(), 1);
    store.save_lifecycle(&mut model).unwrap();
    (model, id)
}
fn evidence(id: GenerationId) -> ValidationEvidence {
    ValidationEvidence {
        generation: id,
        attempt: 1,
        submitted_commit: oid('a'),
        candidate_commit: oid('b'),
        candidate_tree: oid('c'),
        candidate_parent: oid('d'),
        trusted_policy: oid('e'),
        passed: true,
        manifest: vec!["file:src/main.rs".into()],
    }
}
fn landing(evidence: &ValidationEvidence) -> LandingRecord {
    LandingRecord {
        generation: evidence.generation.clone(),
        attempt: evidence.attempt,
        commit: evidence.candidate_commit.clone(),
        revision: 1,
    }
}

#[test]
fn version_and_schema_failures_do_not_repair_database() {
    for mutation in [
        "UPDATE schema_version SET version=99",
        "DROP TABLE dependencies",
        "DROP TABLE schema_version",
        "ALTER TABLE metadata ADD COLUMN unexpected TEXT",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        drop(StateStore::create(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(mutation).unwrap();
        drop(conn);
        let original = std::fs::read(&path).unwrap();
        assert!(StateStore::open(&path).is_err(), "{mutation}");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            original,
            "startup modified invalid schema"
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    std::fs::write(&path, b"not a sqlite database").unwrap();
    assert!(StateStore::open(&path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"not a sqlite database");
}

#[test]
fn metadata_is_validated_and_immutable() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = StateStore::create(temp.path().join("state.db")).unwrap();
    assert!(store.set_baseline("not an oid").is_err());
    assert!(store.set_authority_id("bad\nvalue").is_err());
    store.set_baseline(&oid('d')).unwrap();
    store.set_baseline(&oid('d')).unwrap();
    assert!(store.set_baseline(&oid('e')).is_err());
    store.set_authority_id("authority-1").unwrap();
    assert!(store.set_authority_id("authority-2").is_err());
    assert_eq!(store.baseline().unwrap(), Some(oid('d')));
}

#[test]
fn durable_evidence_is_exact_and_cannot_be_replaced_or_invented() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    let mut store = StateStore::create(&path).unwrap();
    let (mut model, id) = running(&mut store);
    let valid = evidence(id.clone());
    let mut wrong = valid.clone();
    wrong.submitted_commit = oid('f');
    assert!(store.record_evidence(&wrong).is_err());
    wrong = valid.clone();
    wrong.attempt = 2;
    assert!(store.record_evidence(&wrong).is_err());
    assert!(store.record_landing(&landing(&valid)).is_err());
    model
        .complete_attempt(
            &id,
            1,
            Outcome::Landed {
                revision: 1,
                commit: oid('b'),
            },
        )
        .unwrap();
    assert!(store.save_lifecycle(&mut model).is_err());
    store.record_evidence(&valid).unwrap();
    store.record_evidence(&valid).unwrap();
    wrong = valid.clone();
    wrong.passed = false;
    assert!(store.record_evidence(&wrong).is_err());
    let mut wrong_landing = landing(&valid);
    wrong_landing.commit = oid('f');
    assert!(store.record_landing(&wrong_landing).is_err());
    drop(store);
    let store = StateStore::open(path).unwrap();
    assert_eq!(store.evidence(&id, 1).unwrap(), Some(valid));
    assert!(store.landings().unwrap().is_empty());
    assert!(matches!(
        store.load_lifecycle().unwrap().get(&id).unwrap().state(),
        State::Validating { attempt: 1 }
    ));
}

#[test]
fn failed_gate_and_stale_attempt_cannot_land() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = StateStore::create(temp.path().join("state.db")).unwrap();
    let (mut model, id) = running(&mut store);
    let mut failed = evidence(id.clone());
    failed.passed = false;
    store.record_evidence(&failed).unwrap();
    assert!(store.record_landing(&landing(&failed)).is_err());
    model.retry_attempt(&id, 1).unwrap();
    assert_eq!(model.begin_attempt(&id).unwrap(), 2);
    store.save_lifecycle(&mut model).unwrap();
    let mut fresh = evidence(id.clone());
    fresh.attempt = 2;
    store.record_evidence(&fresh).unwrap();
    store.record_landing(&landing(&fresh)).unwrap();
    assert_eq!(store.landings().unwrap()[0].attempt, 2);
    assert!(store.record_landing(&landing(&failed)).is_err());
}

#[test]
fn successful_landing_terminal_and_index_are_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    let mut store = StateStore::create(&path).unwrap();
    let (_, id) = running(&mut store);
    let valid = evidence(id.clone());
    store.record_evidence(&valid).unwrap();
    let landed = landing(&valid);
    store.record_landing(&landed).unwrap();
    store.record_landing(&landed).unwrap();
    assert_eq!(store.unit_revisions("file:src/main.rs").unwrap(), vec![1]);
    let mut wrong = landed.clone();
    wrong.revision = 2;
    assert!(store.record_landing(&wrong).is_err());
    drop(store);
    let raw = Connection::open(&path).unwrap();
    raw.execute("DELETE FROM unit_index", []).unwrap();
    drop(raw);
    let mut store = StateStore::open(&path).unwrap();
    store.rebuild_index().unwrap();
    store.rebuild_index().unwrap();
    assert_eq!(store.landings().unwrap(), vec![landed]);
    assert_eq!(store.unit_revisions("file:src/main.rs").unwrap(), vec![1]);
    assert!(matches!(
        store.load_lifecycle().unwrap().get(&id).unwrap().state(),
        State::Terminal {
            outcome: Outcome::Landed { revision: 1, .. }
        }
    ));
}

#[test]
fn dependencies_and_terminal_outcomes_survive_restart_without_retargeting() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    let mut store = StateStore::create(&path).unwrap();
    let mut model = store.load_lifecycle().unwrap();
    let a = model.register("a", "agent", &oid('a'), vec![]).unwrap();
    let b = model
        .register("b", "agent", &oid('b'), vec![a.clone()])
        .unwrap();
    model.queue(&b).unwrap();
    store.save_lifecycle(&mut model).unwrap();
    model
        .reject(&a, ReasonCode::CheckFailed, Diagnostics::default())
        .unwrap();
    store.save_lifecycle(&mut model).unwrap();
    drop(store);
    let store = StateStore::open(path).unwrap();
    let restored = store.load_lifecycle().unwrap();
    assert_eq!(restored.to_json().unwrap(), model.to_json().unwrap());
    assert_eq!(restored.get(&b).unwrap().dependencies(), &[a]);
    assert!(matches!(
        restored.get(&b).unwrap().state(),
        State::Terminal {
            outcome: Outcome::Rejected {
                reason: ReasonCode::DependencyFailed,
                ..
            }
        }
    ));
}

#[test]
fn malformed_relational_records_refuse_startup() {
    for mutation in [
        "DELETE FROM generations",
        "UPDATE generations SET payload='{}'",
        "INSERT INTO dependencies VALUES('change',1,'missing',1)",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        let mut store = StateStore::create(&path).unwrap();
        running(&mut store);
        drop(store);
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        raw.execute_batch(mutation).unwrap();
        drop(raw);
        assert!(StateStore::open(path).is_err(), "{mutation}");
    }
}

#[test]
fn lifecycle_rollback_cannot_erase_attempts_or_terminal_outcomes() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = StateStore::create(temp.path().join("state.db")).unwrap();
    let (mut model, id) = running(&mut store);
    let mut prior = model.clone();
    model
        .complete_attempt(
            &id,
            1,
            Outcome::Rejected {
                reason: ReasonCode::CheckFailed,
                diagnostics: Diagnostics::default(),
            },
        )
        .unwrap();
    store.save_lifecycle(&mut model).unwrap();
    assert!(store.save_lifecycle(&mut prior).is_err());
}

// Launch this same test executable so process-kill coverage works on native Windows too.
#[test]
#[ignore = "subprocess fixture; invoked by WAL recovery test"]
fn crash_child() {
    let Some(root) = std::env::var_os("MAI_STATE_CRASH_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let path = root.join("state.db");
    let mut store = StateStore::open(&path).unwrap();
    store.set_authority_id("committed-before-kill").unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA synchronous=FULL; BEGIN IMMEDIATE;")
        .unwrap();
    conn.execute("INSERT INTO metadata VALUES('baseline',?1)", [oid('f')])
        .unwrap();
    std::fs::write(root.join("ready"), b"uncommitted transaction open").unwrap();
    thread::sleep(Duration::from_secs(60));
    panic!("parent must kill fixture");
}

#[test]
fn killed_writer_recovers_committed_wal_and_discards_incomplete_transaction() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    let mut store = StateStore::create(&path).unwrap();
    let (model, _) = running(&mut store);
    drop(store);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--ignored", "--nocapture"])
        .env("MAI_STATE_CRASH_ROOT", temp.path())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !temp.path().join("ready").exists() {
        if Instant::now() > deadline || child.try_wait().unwrap().is_some() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("crash fixture did not reach transaction checkpoint");
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(temp.path().join("state.db-wal").exists());
    child.kill().unwrap();
    child.wait().unwrap();
    for _ in 0..3 {
        let store = StateStore::open(&path).unwrap();
        assert_eq!(
            store.authority_id().unwrap().as_deref(),
            Some("committed-before-kill")
        );
        assert_eq!(store.baseline().unwrap(), None);
        assert_eq!(
            store.load_lifecycle().unwrap().to_json().unwrap(),
            model.to_json().unwrap()
        );
    }
}

#[test]
fn malformed_evidence_key_and_payload_refuse_startup() {
    for mutation in [
        "UPDATE evidence SET attempt=2",
        "UPDATE evidence SET payload='{}'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        let mut store = StateStore::create(&path).unwrap();
        let (_, id) = running(&mut store);
        store.record_evidence(&evidence(id)).unwrap();
        drop(store);
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch(mutation).unwrap();
        drop(raw);
        assert!(StateStore::open(path).is_err(), "{mutation}");
    }
}

#[test]
fn evidence_rejects_bad_object_ids_policy_and_duplicate_units() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = StateStore::create(temp.path().join("state.db")).unwrap();
    let (_, id) = running(&mut store);
    let original = evidence(id);
    for field in [
        "candidate_commit",
        "candidate_tree",
        "candidate_parent",
        "trusted_policy",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[field] = serde_json::Value::String("0".repeat(40));
        let invalid = serde_json::from_value(value).unwrap();
        assert!(store.record_evidence(&invalid).is_err(), "{field}");
    }
    let mut invalid = original.clone();
    invalid.manifest.push(invalid.manifest[0].clone());
    assert!(store.record_evidence(&invalid).is_err());
    let mut invalid = original.clone();
    invalid.manifest.push(String::new());
    assert!(store.record_evidence(&invalid).is_err());
    store.record_evidence(&original).unwrap();
}

#[test]
fn stale_snapshot_cannot_clear_pending_approval() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = StateStore::create(temp.path().join("state.db")).unwrap();
    let mut model = store.load_lifecycle().unwrap();
    let id = model
        .register("change", "agent", &oid('a'), vec![])
        .unwrap();
    model.queue(&id).unwrap();
    store.save_lifecycle(&mut model).unwrap();
    let mut stale = store.load_lifecycle().unwrap();
    model.require_approval(&id).unwrap();
    store.save_lifecycle(&mut model).unwrap();
    assert!(store.save_lifecycle(&mut stale).is_err());
    let mut current = store.load_lifecycle().unwrap();
    assert!(matches!(
        current.get(&id).unwrap().state(),
        State::AwaitingApproval
    ));
    assert!(current.begin_attempt(&id).is_err());
    current.approve(&id).unwrap();
    store.save_lifecycle(&mut current).unwrap();
    current.begin_attempt(&id).unwrap();
    store.save_lifecycle(&mut current).unwrap();
}

#[test]
fn landing_cannot_be_rebound_to_interrupted_attempt_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.db");
    let mut store = StateStore::create(&path).unwrap();
    let (mut model, id) = running(&mut store);
    let first = evidence(id.clone());
    store.record_evidence(&first).unwrap();
    model.retry_attempt(&id, 1).unwrap();
    model.begin_attempt(&id).unwrap();
    store.save_lifecycle(&mut model).unwrap();
    assert!(store.record_landing(&landing(&first)).is_err());
    let mut second = first;
    second.attempt = 2;
    store.record_evidence(&second).unwrap();
    store.record_landing(&landing(&second)).unwrap();
    drop(store);
    let conn = Connection::open(&path).unwrap();
    conn.execute("UPDATE landings SET attempt=1", []).unwrap();
    drop(conn);
    assert!(StateStore::open(path).is_err());
}

#[test]
fn manifest_requires_canonical_bytes_and_raw_byte_order() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = StateStore::create(temp.path().join("state.db")).unwrap();
    let (_, id) = running(&mut store);
    let base = evidence(id);
    for units in [
        vec!["file:%61"],
        vec!["file:%ff"],
        vec!["file:"],
        vec!["file:a/../b"],
        vec!["file:a%00b"],
        vec!["file:a b"],
        vec!["file:%FF", "file:z"],
        vec!["file:a", "file:a"],
    ] {
        let mut bad = base.clone();
        bad.manifest = units.into_iter().map(String::from).collect();
        assert!(store.record_evidence(&bad).is_err(), "{:?}", bad.manifest);
    }
    let mut valid = base;
    valid.manifest = vec!["file:%0A", "file:a%20b", "file:z", "file:%FF"]
        .into_iter()
        .map(String::from)
        .collect();
    store.record_evidence(&valid).unwrap();
}
