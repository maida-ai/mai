//! Versioned durable state. Git remains the acceptance ledger; this module never writes Git.
//!
//! There are deliberately no migrations. Callers stage a new authority before `create`,
//! and must verify the actual Git ledger before recording a landing or rebuilding notes.
//! WAL plus FULL synchronization retain committed SQLite transactions across tested process
//! interruption; this is not a claim of power-loss durability or a Git/SQLite transaction.
//! Submission objects remain ordinary Git objects. Derived unit indexes can be rebuilt from
//! retained evidence only after the authority independently verifies the acceptance ledger.
//! Receive acknowledgment and the actual landing CAS are implemented in later layers.
use crate::lifecycle::{AttemptStatus, GenerationId, Lifecycle, Outcome, State};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs::OpenOptions as FileOptions, path::Path, time::Duration};

pub const SCHEMA_VERSION: u32 = 1;
const SCHEMA: &str = "
CREATE TABLE schema_version (singleton INTEGER PRIMARY KEY CHECK(singleton=1), version INTEGER NOT NULL);
CREATE TABLE metadata (key TEXT PRIMARY KEY CHECK(key IN ('baseline','authority_id')), value TEXT NOT NULL);
CREATE TABLE lifecycle (singleton INTEGER PRIMARY KEY CHECK(singleton=1), snapshot TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0 AND typeof(revision)='integer'));
CREATE TABLE generations (change_id TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>0), payload TEXT NOT NULL, PRIMARY KEY(change_id,generation));
CREATE TABLE dependencies (change_id TEXT NOT NULL, generation INTEGER NOT NULL, parent_id TEXT NOT NULL, parent_generation INTEGER NOT NULL, PRIMARY KEY(change_id,generation,parent_id,parent_generation), FOREIGN KEY(change_id,generation) REFERENCES generations(change_id,generation), FOREIGN KEY(parent_id,parent_generation) REFERENCES generations(change_id,generation));
CREATE TABLE evidence (change_id TEXT NOT NULL, generation INTEGER NOT NULL, attempt INTEGER NOT NULL CHECK(attempt>0), payload TEXT NOT NULL, PRIMARY KEY(change_id,generation,attempt), FOREIGN KEY(change_id,generation) REFERENCES generations(change_id,generation));
CREATE TABLE landings (revision INTEGER PRIMARY KEY CHECK(revision>0), change_id TEXT NOT NULL, generation INTEGER NOT NULL, attempt INTEGER NOT NULL, commit_id TEXT NOT NULL UNIQUE, UNIQUE(change_id,generation), FOREIGN KEY(change_id,generation,attempt) REFERENCES evidence(change_id,generation,attempt));
CREATE TABLE unit_index (unit TEXT NOT NULL, revision INTEGER NOT NULL, PRIMARY KEY(unit,revision), FOREIGN KEY(revision) REFERENCES landings(revision));
";

/// Immutable, retained evidence for one exact validation attempt, written before CAS.
/// `passed` is an actual gate result, never inferred from matching Git trees.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationEvidence {
    pub generation: GenerationId,
    pub attempt: u64,
    pub submitted_commit: String,
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub candidate_parent: String,
    pub trusted_policy: String,
    pub passed: bool,
    /// Canonical file units in raw path-byte order, verified against Git by the caller.
    pub manifest: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandingRecord {
    pub generation: GenerationId,
    pub attempt: u64,
    pub commit: String,
    pub revision: u64,
}

/// A loaded lifecycle and its compare-and-swap version. Stale snapshots cannot overwrite
/// later approval, cancellation or attempt transitions.
#[derive(Clone)]
pub struct LifecycleSnapshot {
    model: Lifecycle,
    revision: i64,
}
impl std::ops::Deref for LifecycleSnapshot {
    type Target = Lifecycle;
    fn deref(&self) -> &Lifecycle {
        &self.model
    }
}
impl std::ops::DerefMut for LifecycleSnapshot {
    fn deref_mut(&mut self) -> &mut Lifecycle {
        &mut self.model
    }
}

pub struct StateStore {
    connection: Connection,
}

impl StateStore {
    /// Create exclusively. Never overwrite a database or accept a partial previous create.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        FileOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| {
                format!(
                    "state already exists or cannot be created: {}",
                    path.display()
                )
            })?;
        let mut connection = connect(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        tx.execute("INSERT INTO schema_version VALUES(1,?1)", [SCHEMA_VERSION])?;
        tx.execute(
            "INSERT INTO lifecycle VALUES(1,?1,0)",
            [Lifecycle::new().to_json()?],
        )?;
        tx.commit()?;
        let store = Self { connection };
        store.validate()?;
        Ok(store)
    }

    /// Open without CREATE and validate before applying any settings which could repair state.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let store = Self {
            connection: connect(path.as_ref())?,
        };
        store.validate().context("state.db is unsupported or malformed; preserve it for investigation; migrations are unavailable")?;
        store
            .connection
            .pragma_update(None, "synchronous", "FULL")?;
        Ok(store)
    }

    fn validate(&self) -> Result<()> {
        let version: u32 = self
            .connection
            .query_row(
                "SELECT version FROM schema_version WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .context("missing or malformed schema_version")?;
        ensure!(
            version == SCHEMA_VERSION,
            "unsupported state schema {version}; expected {SCHEMA_VERSION}; no migrations are available"
        );
        let expected = Connection::open_in_memory()?;
        expected.execute_batch(SCHEMA)?;
        ensure!(
            schema(&self.connection)? == schema(&expected)?,
            "schema structure differs from version {SCHEMA_VERSION}"
        );
        let mode: String = self
            .connection
            .pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        ensure!(
            mode == "wal",
            "state.db must already use WAL; refusing implicit repair"
        );
        let integrity: String = self
            .connection
            .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "SQLite integrity failure: {integrity}");
        ensure!(
            !self
                .connection
                .prepare("PRAGMA foreign_key_check")?
                .exists([])?,
            "foreign-key integrity failure"
        );
        let model = self.load_lifecycle()?;
        validate_relations(&self.connection, &model)?;
        for (key, value) in self.metadata()? {
            validate_metadata(&key, &value)?;
        }
        for evidence in self.evidence_all()? {
            validate_evidence(&model, &evidence)?;
        }
        for landing in self.landings()? {
            validate_landing(&self.connection, &model, &landing, true)?;
        }
        // Index rows are derived and may be missing after interruption. Incorrect rows are
        // never used by the authority before its verified reconciliation rebuilds them.
        Ok(())
    }

    pub fn load_lifecycle(&self) -> Result<LifecycleSnapshot> {
        let (payload, revision): (String, i64) = self.connection.query_row(
            "SELECT snapshot,revision FROM lifecycle WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(LifecycleSnapshot {
            model: Lifecycle::from_json(&payload)?,
            revision,
        })
    }

    /// Persist lifecycle transitions atomically with relational identity/dependency records.
    /// This cannot manufacture a Landed outcome without an already recorded landing.
    pub fn save_lifecycle(&mut self, model: &mut LifecycleSnapshot) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision: i64 = tx.query_row(
            "SELECT revision FROM lifecycle WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            revision == model.revision,
            "stale lifecycle snapshot; reload before retrying transition"
        );
        save_model(&tx, model)?;
        tx.commit()?;
        model.revision = revision
            .checked_add(1)
            .context("lifecycle revision overflow")?;
        Ok(())
    }

    pub fn baseline(&self) -> Result<Option<String>> {
        self.get_metadata("baseline")
    }
    pub fn authority_id(&self) -> Result<Option<String>> {
        self.get_metadata("authority_id")
    }
    pub fn set_baseline(&mut self, value: &str) -> Result<()> {
        self.set_metadata("baseline", value)
    }
    pub fn set_authority_id(&mut self, value: &str) -> Result<()> {
        self.set_metadata("authority_id", value)
    }

    fn metadata(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .connection
            .prepare("SELECT key,value FROM metadata ORDER BY key")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
    fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }
    fn set_metadata(&mut self, key: &str, value: &str) -> Result<()> {
        validate_metadata(key, value)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior: Option<String> = tx
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(prior) = prior {
            ensure!(prior == value, "immutable {key} does not match");
        } else {
            tx.execute("INSERT INTO metadata VALUES(?1,?2)", params![key, value])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_evidence(&mut self, evidence: &ValidationEvidence) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let model = load_model(&tx)?;
        validate_evidence(&model, evidence)?;
        if let Some(prior) = get_evidence(&tx, &evidence.generation, evidence.attempt)? {
            ensure!(prior == *evidence, "immutable validation evidence differs");
        } else {
            let submission = model
                .get(&evidence.generation)
                .context("unknown generation")?;
            ensure!(
                matches!(submission.state(), State::Validating { attempt } if *attempt == evidence.attempt),
                "evidence must belong to the active validation attempt"
            );
            tx.execute(
                "INSERT INTO evidence VALUES(?1,?2,?3,?4)",
                params![
                    evidence.generation.change_id,
                    number(evidence.generation.generation)?,
                    number(evidence.attempt)?,
                    serde_json::to_string(evidence)?
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn evidence(
        &self,
        generation: &GenerationId,
        attempt: u64,
    ) -> Result<Option<ValidationEvidence>> {
        get_evidence(&self.connection, generation, attempt)
    }
    pub fn evidence_all(&self) -> Result<Vec<ValidationEvidence>> {
        let values = self.connection.prepare("SELECT change_id,generation,attempt,payload FROM evidence ORDER BY change_id,generation,attempt")?
            .query_map([], |r| Ok((r.get::<_,String>(0)?,unsigned(r,1)?,unsigned(r,2)?,r.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|(change_id, generation, attempt, payload)| {
                let evidence: ValidationEvidence = serde_json::from_str(&payload)?;
                ensure!(
                    evidence.generation.change_id == change_id
                        && evidence.generation.generation == generation
                        && evidence.attempt == attempt,
                    "evidence key does not match immutable payload"
                );
                Ok(evidence)
            })
            .collect()
    }
    pub fn landings(&self) -> Result<Vec<LandingRecord>> {
        get_landings(&self.connection)
    }

    /// Call only after independently verifying Git's exact commit, tree, parent, trailers,
    /// trusted policy and manifest. Commits the terminal outcome and derived index together.
    pub fn record_landing(&mut self, landing: &LandingRecord) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut model = load_model(&tx)?;
        validate_landing(&tx, &model, landing, false)?;
        let existing = get_landings(&tx)?.into_iter().find(|v| {
            v.generation == landing.generation
                || v.revision == landing.revision
                || v.commit == landing.commit
        });
        if let Some(existing) = existing {
            ensure!(
                existing == *landing,
                "conflicting immutable landing association"
            );
        } else {
            model.complete_attempt(
                &landing.generation,
                landing.attempt,
                Outcome::Landed {
                    revision: landing.revision,
                    commit: landing.commit.clone(),
                },
            )?;
            tx.execute(
                "INSERT INTO landings VALUES(?1,?2,?3,?4,?5)",
                params![
                    number(landing.revision)?,
                    landing.generation.change_id,
                    number(landing.generation.generation)?,
                    number(landing.attempt)?,
                    landing.commit
                ],
            )?;
            save_model(&tx, &model)?;
        }
        rebuild_index(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Rebuild only after the caller has verified all stored landings against Git.
    pub fn rebuild_index(&mut self) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        rebuild_index(&tx)?;
        tx.commit()?;
        Ok(())
    }
    pub fn unit_revisions(&self, unit: &str) -> Result<Vec<u64>> {
        Ok(self
            .connection
            .prepare("SELECT revision FROM unit_index WHERE unit=?1 ORDER BY revision")?
            .query_map([unit], |r| unsigned(r, 0))?
            .collect::<rusqlite::Result<_>>()?)
    }
}

fn connect(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("cannot open existing state database {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(connection)
}
fn schema(connection: &Connection) -> Result<Vec<(String, String, String)>> {
    Ok(connection.prepare("SELECT type,name,sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name")?
        .query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<_>>()?)
}
fn number(value: u64) -> Result<i64> {
    i64::try_from(value).context("integer exceeds state schema range")
}
fn oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && value.bytes().any(|c| c != b'0')
}
fn validate_metadata(key: &str, value: &str) -> Result<()> {
    match key {
        "baseline" => ensure!(oid(value), "invalid baseline object ID"),
        "authority_id" => ensure!(
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
            "invalid authority ID"
        ),
        _ => bail!("unknown metadata key"),
    }
    Ok(())
}
fn load_model(connection: &Connection) -> Result<Lifecycle> {
    let payload: String = connection.query_row(
        "SELECT snapshot FROM lifecycle WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    Lifecycle::from_json(&payload)
}
fn save_model(tx: &Transaction<'_>, model: &Lifecycle) -> Result<()> {
    model.validate()?;
    let previous = load_model(tx)?;
    for old in previous.submissions() {
        let new = model
            .get(old.id())
            .context("cannot remove a registered generation")?;
        ensure!(
            old.agent() == new.agent()
                && old.commit() == new.commit()
                && old.dependencies() == new.dependencies(),
            "immutable submitted identity differs"
        );
        ensure!(
            old.predecessor() == new.predecessor(),
            "immutable predecessor differs"
        );
        if matches!(old.state(), State::Terminal { .. }) {
            ensure!(old.state() == new.state(), "terminal outcome is immutable");
        }
        ensure!(
            new.annotations().starts_with(old.annotations()),
            "annotations must be append-only"
        );
        ensure!(
            new.attempts().len() >= old.attempts().len(),
            "attempt history cannot shrink"
        );
        for (before, after) in old.attempts().iter().zip(new.attempts()) {
            ensure!(
                before.number == after.number
                    && (before.status == AttemptStatus::Running || before.status == after.status),
                "completed attempt history is immutable"
            );
        }
    }
    for submission in model.submissions() {
        if let State::Terminal {
            outcome: Outcome::Landed { revision, commit },
        } = submission.state()
        {
            ensure!(
                get_landings(tx)?
                    .iter()
                    .any(|v| &v.generation == submission.id()
                        && v.revision == *revision
                        && v.commit == *commit),
                "Landed requires verified durable landing association"
            );
        }
        tx.execute("INSERT INTO generations VALUES(?1,?2,?3) ON CONFLICT(change_id,generation) DO UPDATE SET payload=excluded.payload", params![submission.id().change_id, number(submission.id().generation)?, serde_json::to_string(submission)?])?;
    }
    tx.execute("DELETE FROM dependencies", [])?;
    for submission in model.submissions() {
        for parent in submission.dependencies() {
            tx.execute(
                "INSERT INTO dependencies VALUES(?1,?2,?3,?4)",
                params![
                    submission.id().change_id,
                    number(submission.id().generation)?,
                    parent.change_id,
                    number(parent.generation)?
                ],
            )?;
        }
    }
    tx.execute(
        "UPDATE lifecycle SET snapshot=?1, revision=revision+1 WHERE singleton=1",
        [model.to_json()?],
    )?;
    Ok(())
}
fn validate_relations(connection: &Connection, model: &Lifecycle) -> Result<()> {
    let mut expected = Vec::new();
    let mut edges = BTreeSet::new();
    for value in model.submissions() {
        expected.push((
            value.id().change_id.clone(),
            number(value.id().generation)?,
            serde_json::to_string(value)?,
        ));
        for parent in value.dependencies() {
            edges.insert((
                value.id().change_id.clone(),
                number(value.id().generation)?,
                parent.change_id.clone(),
                number(parent.generation)?,
            ));
        }
    }
    expected.sort();
    let stored = connection
        .prepare(
            "SELECT change_id,generation,payload FROM generations ORDER BY change_id,generation",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        expected == stored,
        "generation records disagree with lifecycle snapshot"
    );
    let stored_edges = connection
        .prepare("SELECT change_id,generation,parent_id,parent_generation FROM dependencies")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    ensure!(
        edges == stored_edges,
        "dependency records disagree with lifecycle snapshot"
    );
    for record in get_landings(connection)? {
        validate_landing(connection, model, &record, true)?;
    }
    for submission in model.submissions() {
        if let State::Terminal {
            outcome: Outcome::Landed { revision, commit },
        } = submission.state()
        {
            ensure!(
                get_landings(connection)?
                    .iter()
                    .any(|v| &v.generation == submission.id()
                        && v.revision == *revision
                        && v.commit == *commit),
                "terminal landing lacks durable association"
            );
        }
    }
    Ok(())
}
fn validate_evidence(model: &Lifecycle, evidence: &ValidationEvidence) -> Result<()> {
    let submission = model
        .get(&evidence.generation)
        .context("evidence references an unknown generation")?;
    ensure!(
        submission.commit() == evidence.submitted_commit,
        "evidence submitted commit does not match exact generation"
    );
    ensure!(
        submission
            .attempts()
            .iter()
            .any(|v| v.number == evidence.attempt),
        "evidence references unknown attempt"
    );
    ensure!(
        oid(&evidence.candidate_commit)
            && oid(&evidence.candidate_tree)
            && oid(&evidence.candidate_parent),
        "invalid candidate Git object ID"
    );
    ensure!(
        (evidence.trusted_policy.len() == 40 || evidence.trusted_policy.len() == 64)
            && evidence
                .trusted_policy
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            && evidence.trusted_policy.bytes().any(|c| c != b'0'),
        "invalid trusted policy hash"
    );
    let mut previous: Option<Vec<u8>> = None;
    for unit in &evidence.manifest {
        let path = canonical_path(unit)?;
        ensure!(
            previous.as_ref().is_none_or(|previous| previous < &path),
            "manifest must have unique file units sorted by raw path bytes"
        );
        previous = Some(path);
    }
    Ok(())
}
fn get_evidence(
    connection: &Connection,
    generation: &GenerationId,
    attempt: u64,
) -> Result<Option<ValidationEvidence>> {
    let value: Option<String> = connection
        .query_row(
            "SELECT payload FROM evidence WHERE change_id=?1 AND generation=?2 AND attempt=?3",
            params![
                generation.change_id,
                number(generation.generation)?,
                number(attempt)?
            ],
            |r| r.get(0),
        )
        .optional()?;
    value
        .map(|v| {
            let evidence: ValidationEvidence = serde_json::from_str(&v)?;
            ensure!(
                evidence.generation == *generation && evidence.attempt == attempt,
                "evidence key does not match immutable payload"
            );
            Ok(evidence)
        })
        .transpose()
}
fn get_landings(connection: &Connection) -> Result<Vec<LandingRecord>> {
    Ok(connection.prepare("SELECT change_id,generation,attempt,commit_id,revision FROM landings ORDER BY revision")?
        .query_map([], |r| Ok(LandingRecord {generation:GenerationId{change_id:r.get(0)?,generation:unsigned(r,1)?},attempt:unsigned(r,2)?,commit:r.get(3)?,revision:unsigned(r,4)?}))?.collect::<rusqlite::Result<_>>()?)
}
fn validate_landing(
    connection: &Connection,
    model: &Lifecycle,
    landing: &LandingRecord,
    terminal: bool,
) -> Result<()> {
    ensure!(
        landing.revision > 0 && oid(&landing.commit),
        "invalid landing identity"
    );
    let evidence = get_evidence(connection, &landing.generation, landing.attempt)?
        .context("landing has no durable validation evidence")?;
    validate_evidence(model, &evidence)?;
    ensure!(evidence.passed, "landing has no passing gate result");
    ensure!(
        evidence.candidate_commit == landing.commit,
        "landing commit differs from validated candidate"
    );
    if terminal {
        let submission = model
            .get(&landing.generation)
            .context("unknown landed generation")?;
        ensure!(
            submission
                .attempts()
                .last()
                .is_some_and(|attempt| attempt.number == landing.attempt
                    && attempt.status == AttemptStatus::Completed),
            "landing does not match final completed attempt"
        );
        ensure!(
            matches!(submission.state(),State::Terminal { outcome:Outcome::Landed{revision,commit}} if *revision == landing.revision && *commit == landing.commit),
            "landing record differs from terminal outcome"
        );
    }
    Ok(())
}
fn rebuild_index(tx: &Transaction<'_>) -> Result<()> {
    let model = load_model(tx)?;
    let landings = get_landings(tx)?;
    for landing in &landings {
        validate_landing(tx, &model, landing, true)?;
    }
    tx.execute("DELETE FROM unit_index", [])?;
    for landing in landings {
        let evidence = get_evidence(tx, &landing.generation, landing.attempt)?
            .context("missing index evidence")?;
        for unit in evidence.manifest {
            tx.execute(
                "INSERT INTO unit_index VALUES(?1,?2)",
                params![unit, number(landing.revision)?],
            )?;
        }
    }
    Ok(())
}

fn unsigned(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}

// The persistence boundary validates ADR 0004 syntax. Git diff truth is checked by
// authority reconciliation; valid syntax alone never authorizes a manifest or landing.
pub(crate) fn canonical_path(unit: &str) -> Result<Vec<u8>> {
    let encoded = unit
        .strip_prefix("file:")
        .context("manifest unit is not a file unit")?
        .as_bytes();
    let literal =
        |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/');
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < encoded.len() {
        let byte = encoded[i];
        if byte == b'%' {
            ensure!(i + 2 < encoded.len(), "incomplete percent escape");
            let hex = |b: u8| -> Result<u8> {
                match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'A'..=b'F' => Ok(b - b'A' + 10),
                    _ => bail!("noncanonical percent escape"),
                }
            };
            let byte = (hex(encoded[i + 1])? << 4) | hex(encoded[i + 2])?;
            ensure!(!literal(byte), "escaped literal is noncanonical");
            decoded.push(byte);
            i += 3;
        } else {
            ensure!(literal(byte), "unescaped byte in canonical unit");
            decoded.push(byte);
            i += 1;
        }
    }
    ensure!(
        !decoded.is_empty()
            && !decoded.contains(&0)
            && decoded
                .split(|b| *b == b'/')
                .all(|part| !part.is_empty() && part != b"." && part != b".."),
        "invalid repository-relative file unit"
    );
    Ok(decoded)
}
