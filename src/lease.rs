//! Durable ordinary file leases, without priority or preemption.
//!
//! Only the authority engine may mutate this store. Hold its exclusive writer lock
//! from `check_manifest` through landing CAS; SQLite alone cannot lock a Git update.
//! Timestamps are trusted engine Unix milliseconds, never client-provided clocks.
//! A persisted high-water mark rejects clock rollback rather than reviving leases.
//! Expired/released rows remain as generation tombstones. WAL/FULL protects committed
//! SQLite updates across process interruption, not a shared Git/SQLite transaction.
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::OpenOptions, path::Path, time::Duration};

pub const SCHEMA_VERSION: u32 = 1;
const SCHEMA: &str = "
CREATE TABLE lease_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1), version INTEGER NOT NULL, clock_ms INTEGER NOT NULL CHECK(clock_ms>=0));
CREATE TABLE leases (unit TEXT PRIMARY KEY, owner TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>0), expires_ms INTEGER NOT NULL CHECK(expires_ms>=0), released INTEGER NOT NULL CHECK(released IN (0,1)));
";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseClaim {
    pub unit: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    pub unit: String,
    pub owner: String,
    pub generation: u64,
    pub expires_ms: i64,
}

pub struct LeaseStore {
    connection: Connection,
}

impl LeaseStore {
    /// Exclusive creation; incomplete databases are never silently repaired.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.as_ref())?;
        let mut connection = connect(path.as_ref())?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        tx.execute("INSERT INTO lease_meta VALUES(1,?1,0)", [SCHEMA_VERSION])?;
        tx.commit()?;
        Ok(Self { connection })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = connect(path.as_ref())?;
        let tx = connection.unchecked_transaction()?;
        let expected = Connection::open_in_memory()?;
        expected.execute_batch(SCHEMA)?;
        ensure!(
            schema(&connection)? == schema(&expected)?,
            "unsupported lease schema; no migrations available"
        );
        let (version, clock): (u32, i64) = connection.query_row(
            "SELECT version,clock_ms FROM lease_meta WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        ensure!(
            version == SCHEMA_VERSION && clock >= 0,
            "unsupported or malformed lease metadata"
        );
        let integrity: String = connection.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "malformed lease database: {integrity}");
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        ensure!(mode == "wal", "lease database must already use WAL");
        {
            let mut statement = connection
                .prepare("SELECT unit,owner,generation,expires_ms,released FROM leases")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let unit: String = row.get(0)?;
                let owner: String = row.get(1)?;
                let generation: i64 = row.get(2)?;
                let expiry: i64 = row.get(3)?;
                let released: i64 = row.get(4)?;
                crate::state::canonical_path(&unit)?;
                validate_owner(&owner)?;
                ensure!(
                    generation > 0 && expiry >= 0 && matches!(released, 0 | 1),
                    "malformed lease row"
                );
            }
        }
        tx.commit()?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self { connection })
    }

    // Persist observed time independently of a rejected ownership operation. A
    // failed expired claim must not become valid again after a clock rollback.
    fn observe_clock(&mut self, now_ms: i64) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now_ms)?;
        tx.commit()?;
        Ok(())
    }

    /// Acquire an unleased or expired file. Even the same owner's reacquisition
    /// requires expiration/release; use renew to retain an active generation.
    pub fn acquire(
        &mut self,
        unit: &str,
        owner: &str,
        now_ms: i64,
        duration_ms: i64,
    ) -> Result<Lease> {
        crate::state::canonical_path(unit)?;
        validate_owner(owner)?;
        let expires_ms = expiry(now_ms, duration_ms)?;
        self.observe_clock(now_ms)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now_ms)?;
        let previous = row(&tx, unit)?;
        ensure!(
            !previous
                .as_ref()
                .is_some_and(|(lease, released)| !released && lease.expires_ms > now_ms),
            "file already leased: {unit}"
        );
        let generation = previous.map_or(Ok(1), |(lease, _)| {
            lease
                .generation
                .checked_add(1)
                .context("lease generation exhausted")
        })?;
        let generation_sql = i64::try_from(generation).context("lease generation exhausted")?;
        tx.execute("INSERT INTO leases VALUES(?1,?2,?3,?4,0) ON CONFLICT(unit) DO UPDATE SET owner=excluded.owner,generation=excluded.generation,expires_ms=excluded.expires_ms,released=0", params![unit,owner,generation_sql,expires_ms])?;
        tx.commit()?;
        Ok(Lease {
            unit: unit.into(),
            owner: owner.into(),
            generation,
            expires_ms,
        })
    }

    pub fn renew(
        &mut self,
        owner: &str,
        claim: &LeaseClaim,
        now_ms: i64,
        duration_ms: i64,
    ) -> Result<Lease> {
        let expires_ms = expiry(now_ms, duration_ms)?;
        self.observe_clock(now_ms)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now_ms)?;
        let mut lease = held(&tx, owner, claim, now_ms)?;
        ensure!(expires_ms > lease.expires_ms, "renewal must extend expiry");
        tx.execute(
            "UPDATE leases SET expires_ms=?1 WHERE unit=?2",
            params![expires_ms, claim.unit],
        )?;
        tx.commit()?;
        lease.expires_ms = expires_ms;
        Ok(lease)
    }

    pub fn release(&mut self, owner: &str, claim: &LeaseClaim, now_ms: i64) -> Result<()> {
        self.observe_clock(now_ms)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now_ms)?;
        held(&tx, owner, claim, now_ms)?;
        tx.execute("UPDATE leases SET released=1 WHERE unit=?1", [&claim.unit])?;
        tx.commit()?;
        Ok(())
    }

    /// Return only active ownership. Query advances the durable clock watermark.
    pub fn query(&mut self, unit: &str, now_ms: i64) -> Result<Option<Lease>> {
        crate::state::canonical_path(unit)?;
        self.observe_clock(now_ms)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now_ms)?;
        let lease = row(&tx, unit)?
            .filter(|(lease, released)| !released && lease.expires_ms > now_ms)
            .map(|(lease, _)| lease);
        tx.commit()?;
        Ok(lease)
    }

    /// Check actual engine-computed affected files, including regeneration. Unleased
    /// files are allowed; active ownership requires a matching claim. All supplied
    /// claims must still be held, even when a candidate no longer touches that file.
    pub fn check_manifest(
        &mut self,
        owner: &str,
        units: &[String],
        claims: &[LeaseClaim],
        now_ms: i64,
    ) -> Result<()> {
        validate_owner(owner)?;
        self.observe_clock(now_ms)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now_ms)?;
        let mut claimed = BTreeMap::new();
        for claim in claims {
            ensure!(
                claimed.insert(&claim.unit, claim.generation).is_none(),
                "duplicate lease claim"
            );
            held(&tx, owner, claim, now_ms)?;
        }
        for unit in units {
            crate::state::canonical_path(unit)?;
            if let Some((lease, false)) = row(&tx, unit)? {
                if lease.expires_ms > now_ms {
                    ensure!(
                        lease.owner == owner && claimed.get(unit) == Some(&lease.generation),
                        "lease ownership or generation mismatch: {unit}"
                    );
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn validate_owner(owner: &str) -> Result<()> {
    ensure!(
        !owner.is_empty()
            && owner.len() <= 128
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "invalid lease owner"
    );
    Ok(())
}
fn expiry(now_ms: i64, duration_ms: i64) -> Result<i64> {
    ensure!(
        now_ms >= 0 && duration_ms > 0,
        "lease clock must be nonnegative and duration positive"
    );
    now_ms
        .checked_add(duration_ms)
        .context("lease expiry overflow")
}
fn clock(tx: &Transaction<'_>, now_ms: i64) -> Result<()> {
    let previous: i64 = tx.query_row(
        "SELECT clock_ms FROM lease_meta WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        now_ms >= previous,
        "engine clock moved backwards; lease operation refused"
    );
    tx.execute(
        "UPDATE lease_meta SET clock_ms=?1 WHERE singleton=1",
        [now_ms],
    )?;
    Ok(())
}
fn held(tx: &Transaction<'_>, owner: &str, claim: &LeaseClaim, now_ms: i64) -> Result<Lease> {
    validate_owner(owner)?;
    crate::state::canonical_path(&claim.unit)?;
    let (lease, released) = row(tx, &claim.unit)?.context("lease does not exist")?;
    ensure!(
        !released
            && lease.expires_ms > now_ms
            && lease.owner == owner
            && lease.generation == claim.generation,
        "lease expired, released, or ownership/generation mismatch: {}",
        claim.unit
    );
    Ok(lease)
}
fn row(tx: &Transaction<'_>, unit: &str) -> Result<Option<(Lease, bool)>> {
    Ok(tx
        .query_row(
            "SELECT owner,generation,expires_ms,released FROM leases WHERE unit=?1",
            [unit],
            |r| {
                let generation: i64 = r.get(1)?;
                let generation = u64::try_from(generation)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, generation))?;
                Ok((
                    Lease {
                        unit: unit.into(),
                        owner: r.get(0)?,
                        generation,
                        expires_ms: r.get(2)?,
                    },
                    r.get(3)?,
                ))
            },
        )
        .optional()?)
}
fn connect(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_secs(5))?;
    Ok(connection)
}
fn schema(connection: &Connection) -> Result<Vec<(String, String, String)>> {
    Ok(connection.prepare("SELECT type,name,sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name")?.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<_>>()?)
}
