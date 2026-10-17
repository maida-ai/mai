//! Attributable trusted-local administration. Actor strings are not authentication.
use crate::policy::Binding;
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approved,
    Denied,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub schema_version: u32,
    pub binding: Binding,
    pub decision: Decision,
    pub actor: String,
    pub reason: String,
    pub recorded_ms: u64,
}
pub struct ApprovalStore(Connection);
impl ApprovalStore {
    pub fn open(root: &Path) -> Result<Self> {
        let path = root.join(".mai/gates.db");
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            ensure!(
                !meta.file_type().is_symlink(),
                "gate store must not be a symlink"
            );
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            version == 0 || version == 1,
            "unsupported gate store version; no migration is supported"
        );
        if version == 0 {
            let tables: u32 = connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )?;
            ensure!(tables == 0, "unversioned gate store is not empty");
            connection.execute_batch("BEGIN IMMEDIATE; CREATE TABLE approvals(id INTEGER PRIMARY KEY, binding TEXT NOT NULL, payload TEXT NOT NULL); CREATE TABLE results(id INTEGER PRIMARY KEY, payload TEXT NOT NULL); PRAGMA user_version=1; COMMIT;")?;
        }
        Ok(Self(connection))
    }
    /// Administrative callers must hold authority ownership through publication.
    pub fn record(
        &mut self,
        binding: &Binding,
        decision: Decision,
        actor: &str,
        reason: &str,
    ) -> Result<Approval> {
        ensure!(
            !actor.trim().is_empty() && !reason.trim().is_empty(),
            "approval administration requires explicit actor and reason"
        );
        ensure!(
            actor.len() <= 1024 && reason.len() <= 8192,
            "approval attribution exceeds size limit"
        );
        ensure!(
            binding.schema_version == 1,
            "unsupported approval binding version"
        );
        ensure!(
            binding.submitted_identity.as_deref() != Some(actor),
            "a submitted identity cannot approve or deny its own candidate"
        );
        let record = Approval {
            schema_version: 1,
            binding: binding.clone(),
            decision,
            actor: actor.to_owned(),
            reason: reason.to_owned(),
            recorded_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_millis()
                .try_into()?,
        };
        self.0.execute(
            "INSERT INTO approvals(binding,payload) VALUES(?1,?2)",
            params![
                serde_json::to_string(binding)?,
                serde_json::to_string(&record)?
            ],
        )?;
        Ok(record)
    }
    pub fn decision(&self, binding: &Binding) -> Result<Option<Approval>> {
        let payload: Option<String> = self
            .0
            .query_row(
                "SELECT payload FROM approvals WHERE binding=?1 ORDER BY id DESC LIMIT 1",
                [serde_json::to_string(binding)?],
                |r| r.get(0),
            )
            .optional()?;
        payload
            .map(|text| {
                let value: Approval = serde_json::from_str(&text)?;
                ensure!(
                    value.schema_version == 1 && value.binding == *binding,
                    "unsupported or inconsistent approval record"
                );
                Ok(value)
            })
            .transpose()
    }
    pub fn record_result(&mut self, result: &impl Serialize) -> Result<()> {
        self.0.execute(
            "INSERT INTO results(payload) VALUES(?1)",
            [serde_json::to_string(result)?],
        )?;
        Ok(())
    }
}
