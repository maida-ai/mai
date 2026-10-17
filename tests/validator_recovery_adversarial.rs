//! Recovery proof is claim-specific, replayable, and never a stale-lock heuristic.
#![cfg(target_os = "linux")]

use mai::{
    native_path::NativePath,
    policy::Binding,
    process::{CancellationToken, ProcessOutcome, RunSpec},
    slots::{SlotPool, SlotSettings},
};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};

struct Fixture {
    _root: tempfile::TempDir,
    pool: SlotPool,
    slot: PathBuf,
    source: PathBuf,
    binding: Binding,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let pool_root = root.path().join("pool");
        let pool = SlotPool::open(
            &pool_root,
            &source,
            SlotSettings {
                count: 1,
                port_base: 37000,
                ports_per_slot: 1,
                caches: vec![],
                supervised_recovery: true,
            },
        )
        .unwrap();
        let binding = Binding {
            schema_version: 1,
            authority: NativePath::from_path(&source).unwrap(),
            candidate: "a".repeat(40),
            tree: "b".repeat(40),
            base: "c".repeat(40),
            policy: "c".repeat(40),
            submitted_identity: None,
        };
        Self {
            _root: root,
            pool,
            slot: pool_root.join("slot-0"),
            source,
            binding,
        }
    }

    fn interrupted(&self) -> String {
        let mut lease = self.pool.acquire_bound(Some(&self.binding)).unwrap();
        let cancel = CancellationToken::default();
        cancel.cancel();
        let result = lease
            .run(
                &RunSpec {
                    program: "sh".into(),
                    args: vec!["-c".into(), "exit 0".into()],
                    cwd: self.source.clone(),
                    env: Default::default(),
                    timeout: Duration::from_secs(1),
                    output_tail_bytes: 1024,
                },
                &cancel,
            )
            .unwrap();
        assert_eq!(result.outcome, ProcessOutcome::Cancelled);
        fs::create_dir_all(lease.path()).unwrap();
        fs::write(lease.path().join("evidence"), b"preserve me").unwrap();
        lease
            .retain_after_verified_cancellation("fixture-worker".into())
            .unwrap();
        self.claim()["claim_id"].as_str().unwrap().to_owned()
    }

    fn claim(&self) -> Value {
        serde_json::from_slice(&fs::read(self.slot.join("occupied")).unwrap()).unwrap()
    }

    fn receipt(&self) -> Value {
        serde_json::from_slice(&fs::read(self.slot.join("cleanup-receipt.json")).unwrap()).unwrap()
    }
}

#[test]
fn receipt_requires_verified_cancellation_and_cannot_certify_an_unused_lease() {
    let fixture = Fixture::new();
    let lease = fixture.pool.acquire_bound(Some(&fixture.binding)).unwrap();
    assert!(
        lease
            .retain_after_verified_cancellation("no-native-proof".into())
            .is_err(),
        "receipt production must enforce its cleanup precondition"
    );
    assert!(fixture.slot.join("occupied").exists());
    assert!(!fixture.slot.join("cleanup-receipt.json").exists());
}

#[test]
fn free_lock_without_receipt_preserves_the_interrupted_claim() {
    let fixture = Fixture::new();
    let lease = fixture.pool.acquire_bound(Some(&fixture.binding)).unwrap();
    let claim = fixture.claim();
    drop(lease);
    let before = fs::read(fixture.slot.join("occupied")).unwrap();
    assert!(
        fixture
            .pool
            .recover(0, claim["claim_id"].as_str().unwrap())
            .is_err()
    );
    assert_eq!(fs::read(fixture.slot.join("occupied")).unwrap(), before);
    assert!(fixture.pool.acquire().is_err());
}

#[test]
fn stale_request_refuses_then_exact_recovery_is_idempotent_and_keeps_lock_inode() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let lock_inode = fs::metadata(fixture.slot.join("lock")).unwrap().ino();
    let occupied = fs::read(fixture.slot.join("occupied")).unwrap();
    assert!(fixture.pool.recover(0, "stale-claim-id").is_err());
    assert_eq!(fs::read(fixture.slot.join("occupied")).unwrap(), occupied);
    let quarantine = fixture.pool.recover(0, &claim_id).unwrap();
    assert_eq!(
        fs::read(quarantine.join("candidate/evidence")).unwrap(),
        b"preserve me"
    );
    assert_eq!(
        fs::metadata(fixture.slot.join("lock")).unwrap().ino(),
        lock_inode
    );
    assert_eq!(
        fixture.pool.recover(0, &claim_id).unwrap(),
        quarantine,
        "a lost successful reply must not make exact retry fail"
    );
    let next = fixture.pool.acquire_bound(Some(&fixture.binding)).unwrap();
    let next_claim = fixture.claim();
    assert_ne!(next_claim["claim_id"].as_str().unwrap(), claim_id);
    assert!(fixture.pool.recover(0, &claim_id).is_err());
    drop(next);
    assert_eq!(fixture.claim(), next_claim);
}

#[test]
fn journal_replay_revalidates_all_retained_proof_before_moving_artifacts() {
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let mut receipt = fixture.receipt();
    receipt["boot_id"] = "another-host-boot".into();
    let journal = serde_json::json!({
        "version": 1,
        "claim": fixture.claim(),
        "receipt": receipt,
        "phase": "prepared"
    });
    fs::write(
        fixture.slot.join("recovery-journal.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    assert!(fixture.pool.recover(0, &claim_id).is_err());
    assert_eq!(
        fs::read(fixture.slot.join("candidate/evidence")).unwrap(),
        b"preserve me"
    );
    assert!(fixture.slot.join("occupied").exists());
}

#[test]
fn interrupted_recovery_replays_durable_proof_after_receipt_was_removed() {
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let journal = serde_json::json!({
        "version": 1,
        "claim": fixture.claim(),
        "receipt": fixture.receipt(),
        "phase": "prepared"
    });
    fs::write(
        fixture.slot.join("recovery-journal.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    let quarantine = fixture.slot.join(format!("quarantine-{claim_id}"));
    fs::create_dir(&quarantine).unwrap();
    fs::rename(fixture.slot.join("candidate"), quarantine.join("candidate")).unwrap();
    fs::remove_file(fixture.slot.join("cleanup-receipt.json")).unwrap();
    assert_eq!(fixture.pool.recover(0, &claim_id).unwrap(), quarantine);
    assert!(!fixture.slot.join("occupied").exists());
    assert_eq!(
        fs::read(quarantine.join("candidate/evidence")).unwrap(),
        b"preserve me"
    );
}

#[test]
fn receipt_cannot_certify_a_modified_original_policy_binding() {
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let mut claim = fixture.claim();
    claim["binding"]["candidate"] = "d".repeat(40).into();
    fs::write(
        fixture.slot.join("occupied"),
        serde_json::to_vec(&claim).unwrap(),
    )
    .unwrap();
    assert!(fixture.pool.recover(0, &claim_id).is_err());
    assert!(fixture.slot.join("candidate/evidence").exists());
    assert!(fixture.slot.join("occupied").exists());
}

#[test]
fn temporary_record_cannot_quarantine_another_claims_directory() {
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let unrelated = tempfile::Builder::new()
        .prefix("mai-validator-")
        .tempdir()
        .unwrap();
    fs::write(unrelated.path().join("keep"), b"unrelated evidence").unwrap();
    fs::write(
        fixture.slot.join("temporary.json"),
        serde_json::to_vec(&NativePath::from_path(unrelated.path()).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(fixture.pool.recover(0, &claim_id).is_err());
    assert_eq!(
        fs::read(unrelated.path().join("keep")).unwrap(),
        b"unrelated evidence"
    );
    assert!(fixture.slot.join("occupied").exists());
}

#[test]
fn a_later_failed_spawn_invalidates_the_previous_cancellation_proof() {
    let fixture = Fixture::new();
    let mut lease = fixture.pool.acquire_bound(Some(&fixture.binding)).unwrap();
    let mut spec = RunSpec {
        program: "sh".into(),
        args: vec![],
        cwd: fixture.source.clone(),
        env: Default::default(),
        timeout: Duration::from_secs(1),
        output_tail_bytes: 1024,
    };
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert_eq!(
        lease.run(&spec, &cancelled).unwrap().outcome,
        ProcessOutcome::Cancelled
    );
    spec.program = fixture.source.join("missing-program").into_os_string();
    assert!(lease.run(&spec, &CancellationToken::default()).is_err());
    assert!(
        lease
            .retain_after_verified_cancellation("stale-native-proof".into())
            .is_err()
    );
    assert!(!fixture.slot.join("cleanup-receipt.json").exists());
}

#[test]
fn enabling_current_policy_reopens_pool_without_retroactively_opting_in_a_claim() {
    let fixture = Fixture::new();
    let pool_root = fixture.slot.parent().unwrap();
    let settings = |enabled| SlotSettings {
        count: 1,
        port_base: 37000,
        ports_per_slot: 1,
        caches: vec![],
        supervised_recovery: enabled,
    };
    let disabled = SlotPool::open(pool_root, &fixture.source, settings(false)).unwrap();
    let lease = disabled.acquire_bound(Some(&fixture.binding)).unwrap();
    let claim = fixture.claim();
    assert_eq!(claim["recovery_contract"], "");
    drop(lease);
    let enabled = SlotPool::open(pool_root, &fixture.source, settings(true)).unwrap();
    let error = enabled
        .recover(0, claim["claim_id"].as_str().unwrap())
        .unwrap_err();
    assert!(error.to_string().contains("opt-in"), "{error:#}");
    assert_eq!(fixture.claim(), claim);
}

#[test]
fn slot_directory_symlink_never_redirects_recovery() {
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let retained = fixture.slot.with_file_name("retained-slot");
    fs::rename(&fixture.slot, &retained).unwrap();
    std::os::unix::fs::symlink(&retained, &fixture.slot).unwrap();
    assert!(fixture.pool.recover(0, &claim_id).is_err());
    assert!(retained.join("occupied").exists());
    assert_eq!(
        fs::read(retained.join("candidate/evidence")).unwrap(),
        b"preserve me"
    );
}

#[test]
fn quarantine_directory_symlink_never_redirects_artifact_moves() {
    let fixture = Fixture::new();
    let claim_id = fixture.interrupted();
    let unrelated = fixture.source.join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep"), b"untouched").unwrap();
    std::os::unix::fs::symlink(
        &unrelated,
        fixture.slot.join(format!("quarantine-{claim_id}")),
    )
    .unwrap();
    assert!(fixture.pool.recover(0, &claim_id).is_err());
    assert_eq!(fs::read(unrelated.join("keep")).unwrap(), b"untouched");
    assert!(!unrelated.join("candidate").exists());
    assert!(fixture.slot.join("candidate/evidence").exists());
}
