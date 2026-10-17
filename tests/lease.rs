use mai::lease::{Lease, LeaseClaim, LeaseStore};

fn claim(lease: &Lease) -> LeaseClaim {
    LeaseClaim {
        unit: lease.unit.clone(),
        generation: lease.generation,
    }
}

#[test]
fn exclusive_creation_restart_and_generation_fencing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    assert!(LeaseStore::open(&path, "test-authority").is_err());
    assert!(!path.exists());
    let mut store = LeaseStore::create(&path, "test-authority").unwrap();
    assert!(LeaseStore::create(&path, "test-authority").is_err());
    let first = store.acquire("file:src/a.rs", "alice", 100, 20).unwrap();
    assert_eq!(first.generation, 1);
    assert!(store.acquire(&first.unit, "bob", 101, 20).is_err());
    assert!(store.acquire(&first.unit, "alice", 101, 20).is_err());
    assert!(store.renew("bob", &claim(&first), 101, 30).is_err());
    assert!(store.release("bob", &claim(&first), 101).is_err());
    let renewed = store.renew("alice", &claim(&first), 110, 30).unwrap();
    assert_eq!(renewed.expires_ms, 140);
    drop(store);
    let mut store = LeaseStore::open(&path, "test-authority").unwrap();
    assert_eq!(store.query(&first.unit, 110).unwrap(), Some(renewed));
    store.release("alice", &claim(&first), 111).unwrap();
    assert!(store.query(&first.unit, 111).unwrap().is_none());
    let second = store.acquire(&first.unit, "alice", 112, 50).unwrap();
    assert_eq!(second.generation, 2);
    assert!(store.release("alice", &claim(&first), 113).is_err());
    assert!(store.renew("alice", &claim(&first), 113, 100).is_err());
    assert_eq!(store.query(&first.unit, 113).unwrap(), Some(second));
}

#[test]
fn expiration_is_exact_and_clock_rollback_cannot_revive_a_lease() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    let mut store = LeaseStore::create(&path, "test-authority").unwrap();
    let first = store.acquire("file:a", "alice", 100, 20).unwrap();
    assert!(store.query(&first.unit, 119).unwrap().is_some());
    assert!(store.query(&first.unit, 120).unwrap().is_none());
    assert!(store.renew("alice", &claim(&first), 120, 100).is_err());
    drop(store);
    let mut store = LeaseStore::open(&path, "test-authority").unwrap();
    assert!(store.query(&first.unit, 119).is_err());
    let second = store.acquire(&first.unit, "bob", 120, 20).unwrap();
    assert_eq!(second.generation, 2);
    assert!(store.release("alice", &claim(&first), 120).is_err());
}

#[test]
fn landing_checks_real_units_owner_generation_and_every_claim() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = LeaseStore::create(temp.path().join("leases.db"), "test-authority").unwrap();
    let lease = store.acquire("file:a", "alice", 100, 20).unwrap();
    let units = vec!["file:a".into(), "file:b".into()];
    assert!(store.check_manifest("alice", &units, &[], 101).is_err());
    assert!(
        store
            .check_manifest("bob", &units, &[claim(&lease)], 101)
            .is_err()
    );
    store
        .check_manifest("alice", &units, &[claim(&lease)], 101)
        .unwrap();
    store
        .check_manifest("bob", &["file:b".into()], &[], 101)
        .unwrap();
    let stale = LeaseClaim {
        unit: lease.unit.clone(),
        generation: 10,
    };
    assert!(
        store
            .check_manifest("alice", &units, &[stale], 102)
            .is_err()
    );
    assert!(
        store
            .check_manifest("alice", &units, &[claim(&lease), claim(&lease)], 102)
            .is_err()
    );
    assert!(
        store
            .check_manifest("alice", &[], &[claim(&lease)], 120)
            .is_err()
    );
    store.check_manifest("bob", &units, &[], 120).unwrap();
    let _new = store.acquire("file:a", "bob", 120, 20).unwrap();
    assert!(
        store
            .check_manifest("alice", &units, &[claim(&lease)], 120)
            .is_err()
    );
}

#[test]
fn manifest_reports_only_ownership_predicates_as_typed_lease_blocks() {
    use mai::lease::{LeaseBlockReason, LeaseBlocked};

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    let mut store = LeaseStore::create(&path, "test-authority").unwrap();
    let lease = store.acquire("file:a", "alice", 100, 20).unwrap();
    let stale = LeaseClaim {
        unit: lease.unit.clone(),
        generation: lease.generation + 1,
    };
    let blocked = store
        .check_manifest("alice", &["file:a".into()], &[stale], 101)
        .unwrap_err();
    assert_eq!(
        blocked.downcast_ref::<LeaseBlocked>().unwrap().reason,
        LeaseBlockReason::ClaimIdentityMismatch
    );

    let invalid_owner = store
        .check_manifest("bad owner", &["file:a".into()], &[], 101)
        .unwrap_err();
    assert!(invalid_owner.downcast_ref::<LeaseBlocked>().is_none());

    let invalid_unit = store
        .check_manifest(
            "alice",
            &[],
            &[LeaseClaim {
                unit: "file:a/../b".into(),
                generation: 1,
            }],
            101,
        )
        .unwrap_err();
    assert!(invalid_unit.downcast_ref::<LeaseBlocked>().is_none());

    let clock_rollback = store
        .check_manifest("alice", &["file:a".into()], &[claim(&lease)], 99)
        .unwrap_err();
    assert!(clock_rollback.downcast_ref::<LeaseBlocked>().is_none());

    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE leases")
        .unwrap();
    let database_error = store
        .check_manifest("alice", &[], &[claim(&lease)], 101)
        .unwrap_err();
    assert!(database_error.downcast_ref::<LeaseBlocked>().is_none());
}

#[test]
fn rejects_noncanonical_units_invalid_owners_and_invalid_durations() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = LeaseStore::create(temp.path().join("leases.db"), "test-authority").unwrap();
    for unit in [
        "file:",
        "file:/a",
        "file:a/../b",
        "file:a//b",
        "file:%61",
        "file:%ff",
        "file:%00",
        "symbol:a",
        "file:a b",
    ] {
        assert!(store.acquire(unit, "alice", 100, 20).is_err(), "{unit}");
        assert!(
            store
                .check_manifest("alice", &[unit.into()], &[], 100)
                .is_err()
        );
    }
    for owner in ["", "a b", "a/b"] {
        assert!(store.acquire("file:a", owner, 100, 20).is_err());
    }
    for (now, duration) in [(-1, 10), (0, 0), (0, -1), (i64::MAX, 1)] {
        assert!(store.acquire("file:a", "alice", now, duration).is_err());
    }
    for unit in ["file:%FF", "file:a%20b", "file:%2520", "file:a%5Cb"] {
        store.acquire(unit, "alice", 100, 20).unwrap();
    }
}

#[test]
fn malformed_or_unsupported_database_is_not_repaired() {
    for sql in [
        "UPDATE lease_meta SET version=99",
        "DROP TABLE lease_meta",
        "ALTER TABLE leases ADD COLUMN surprise TEXT",
        "UPDATE leases SET unit='file:%61'",
        "UPDATE leases SET owner='bad owner'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("leases.db");
        let mut store = LeaseStore::create(&path, "test-authority").unwrap();
        store.acquire("file:a", "alice", 100, 20).unwrap();
        drop(store);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch(sql).unwrap();
        drop(connection);
        let before = std::fs::read(&path).unwrap();
        assert!(LeaseStore::open(&path, "test-authority").is_err(), "{sql}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bad.db");
    std::fs::write(&path, b"not sqlite").unwrap();
    assert!(LeaseStore::open(path, "test-authority").is_err());
}

#[test]
fn two_connections_do_not_both_acquire_and_released_generation_survives_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    let mut first = LeaseStore::create(&path, "test-authority").unwrap();
    let mut second = LeaseStore::open(&path, "test-authority").unwrap();
    let lease = first.acquire("file:a", "alice", 1, 10).unwrap();
    assert!(second.acquire("file:a", "bob", 1, 10).is_err());
    second.release("alice", &claim(&lease), 2).unwrap();
    drop(first);
    drop(second);
    let mut store = LeaseStore::open(&path, "test-authority").unwrap();
    assert_eq!(store.acquire("file:a", "bob", 3, 10).unwrap().generation, 2);
}

#[test]
fn process_exit_retains_committed_acquisition() {
    const CHILD: &str = "MAI_LEASE_TEST_CHILD_DB";
    if let Some(path) = std::env::var_os(CHILD) {
        let mut store = LeaseStore::open(path, "test-authority").unwrap();
        store.acquire("file:a", "alice", 100, 20).unwrap();
        // No destructor/checkpoint: exercise WAL recovery after an abrupt exit.
        std::process::exit(0);
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    drop(LeaseStore::create(&path, "test-authority").unwrap());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "process_exit_retains_committed_acquisition",
            "--nocapture",
        ])
        .env(CHILD, &path)
        .status()
        .unwrap();
    assert!(status.success());
    let mut store = LeaseStore::open(&path, "test-authority").unwrap();
    let lease = store.query("file:a", 101).unwrap().unwrap();
    assert_eq!(lease.owner, "alice");
    assert_eq!(lease.generation, 1);
    assert!(store.acquire("file:a", "bob", 101, 20).is_err());
}

#[test]
fn rejected_expired_claim_still_persists_clock_watermark() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    let mut store = LeaseStore::create(&path, "test-authority").unwrap();
    let lease = store.acquire("file:a", "alice", 100, 20).unwrap();
    assert!(
        store
            .check_manifest("alice", &["file:a".into()], &[claim(&lease)], 120)
            .is_err()
    );
    drop(store);
    let mut store = LeaseStore::open(path, "test-authority").unwrap();
    assert!(store.renew("alice", &claim(&lease), 119, 100).is_err());
}

#[test]
fn interrupted_uncommitted_transfer_preserves_previous_owner() {
    const CHILD: &str = "MAI_LEASE_UNCOMMITTED_CHILD_DB";
    if let Some(path) = std::env::var_os(CHILD) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch("BEGIN IMMEDIATE; UPDATE leases SET owner='bob',generation=2;")
            .unwrap();
        std::process::exit(0);
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    let mut store = LeaseStore::create(&path, "test-authority").unwrap();
    let lease = store.acquire("file:a", "alice", 100, 20).unwrap();
    drop(store);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interrupted_uncommitted_transfer_preserves_previous_owner",
            "--nocapture",
        ])
        .env(CHILD, &path)
        .status()
        .unwrap();
    assert!(status.success());
    let mut store = LeaseStore::open(&path, "test-authority").unwrap();
    assert_eq!(store.query("file:a", 101).unwrap(), Some(lease));
    drop(store);
    assert_eq!(
        LeaseStore::open(&path, "test-authority")
            .unwrap()
            .query("file:a", 101)
            .unwrap()
            .unwrap()
            .owner,
        "alice"
    );
}

#[test]
fn lease_identity_and_schema_version_are_required_without_migration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("leases.db");
    drop(LeaseStore::create(&path, "first").unwrap());
    assert!(LeaseStore::open(&path, "second").is_err());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute("UPDATE lease_meta SET version=1", [])
        .unwrap();
    drop(connection);
    let before = std::fs::read(&path).unwrap();
    assert!(LeaseStore::open(&path, "first").is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
