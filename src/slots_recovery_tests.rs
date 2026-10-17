//! Separate-process interruption tests for recovery transitions. The native
//! engine/worker supervision path is qualified in validator_recovery_e2e.rs.
use super::*;
use crate::policy::Binding;
use std::process::Command;

const HELPER: &str = "slots::recovery_crash_tests::recovery_interruption_helper";

thread_local! {
    static CAPTURE_COMPLETED_LOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static COMPLETED_LOCK_DUPLICATE: std::cell::RefCell<Option<File>> = const { std::cell::RefCell::new(None) };
}

/// Hold the same open file description across completed recovery's return,
/// deterministically modeling a concurrent fork that inherited its flock.
pub(super) fn duplicate_completed_lock(lock: &File) {
    if CAPTURE_COMPLETED_LOCK.get() {
        COMPLETED_LOCK_DUPLICATE.with(|duplicate| {
            *duplicate.borrow_mut() = Some(lock.try_clone().unwrap());
        });
    }
}

/// This module and every call to this hook exist only in a cfg(test) build.
/// The helper exits immediately, skipping Rust destructors like an interrupted
/// process, and the parent reopens the same durable claim to finish recovery.
pub(super) fn checkpoint(name: &str) {
    if std::env::var("MAI_TEST_RECOVERY_HELPER").as_deref() == Ok("1")
        && std::env::var("MAI_TEST_RECOVERY_CHECKPOINT").as_deref() == Ok(name)
    {
        std::process::exit(77);
    }
}

fn settings() -> SlotSettings {
    SlotSettings {
        count: 1,
        port_base: 39000,
        ports_per_slot: 1,
        caches: vec![],
        supervised_recovery: true,
    }
}

#[test]
#[ignore = "fixture subprocess helper; invoked by recovery transition tests"]
fn recovery_interruption_helper() {
    assert_eq!(std::env::var("MAI_TEST_RECOVERY_HELPER").unwrap(), "1");
    let root = PathBuf::from(std::env::var_os("MAI_TEST_RECOVERY_ROOT").unwrap());
    let claim = std::env::var("MAI_TEST_RECOVERY_CLAIM").unwrap();
    let pool = SlotPool::open(&root.join("pool"), &root.join("source"), settings()).unwrap();
    pool.recover(0, &claim).unwrap();
    panic!("fixture recovery did not reach its selected interruption point");
}

#[test]
fn recovery_retries_after_separate_process_interruption_at_each_durable_boundary() {
    for stage in ["journal-written", "artifacts-quarantined", "claim-cleared"] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let pool = SlotPool::open(&root.path().join("pool"), &source, settings()).unwrap();
        let binding = Binding {
            schema_version: 1,
            authority: NativePath::from_path(&source).unwrap(),
            candidate: "a".repeat(40),
            tree: "b".repeat(40),
            base: "c".repeat(40),
            policy: "c".repeat(40),
            submitted_identity: None,
        };
        let mut lease = pool.acquire_bound(Some(&binding)).unwrap();
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let result = lease
            .run(
                &RunSpec {
                    program: "sh".into(),
                    args: vec![],
                    cwd: source,
                    env: Default::default(),
                    timeout: std::time::Duration::from_secs(1),
                    output_tail_bytes: 1024,
                },
                &cancellation,
            )
            .unwrap();
        assert_eq!(result.outcome, ProcessOutcome::Cancelled);
        fs::create_dir(lease.path()).unwrap();
        fs::write(lease.path().join("evidence"), b"retained recovery evidence").unwrap();
        lease
            .retain_after_verified_cancellation("recovery-transition-fixture".into())
            .unwrap();
        let slot = root.path().join("pool/slot-0");
        let claim: SlotClaim =
            serde_json::from_slice(&fs::read(slot.join("occupied")).unwrap()).unwrap();
        let lock_inode = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(slot.join("lock")).unwrap().ino()
        };
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", HELPER, "--ignored", "--nocapture"])
            .env("MAI_TEST_RECOVERY_HELPER", "1")
            .env("MAI_TEST_RECOVERY_ROOT", root.path())
            .env("MAI_TEST_RECOVERY_CLAIM", &claim.claim_id)
            .env("MAI_TEST_RECOVERY_CHECKPOINT", stage)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(77),
            "stage={stage} stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(slot.join("occupied").exists(), stage != "claim-cleared");
        let recovered = pool.recover(0, &claim.claim_id).unwrap();
        assert_eq!(
            fs::read(recovered.join("candidate/evidence")).unwrap(),
            b"retained recovery evidence",
            "stage={stage}"
        );
        assert!(!slot.join("occupied").exists());
        CAPTURE_COMPLETED_LOCK.set(true);
        assert_eq!(pool.recover(0, &claim.claim_id).unwrap(), recovered);
        CAPTURE_COMPLETED_LOCK.set(false);
        COMPLETED_LOCK_DUPLICATE.with(|duplicate| assert!(duplicate.borrow().is_some()));
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(fs::metadata(slot.join("lock")).unwrap().ino(), lock_inode);
        }
        pool.acquire_bound(Some(&binding))
            .unwrap()
            .release()
            .unwrap();
        COMPLETED_LOCK_DUPLICATE.with(|duplicate| drop(duplicate.borrow_mut().take()));
    }
}
