//! Agent-facing publication receipt, independent of CLI formatting.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub revision: u64,
    pub commit: String,
    pub agent: String,
    pub files: Vec<String>,
    pub rebased: bool,
    pub recovered: bool,
    pub workspace_updated: bool,
    pub retries: u32,
    pub lock_wait_ms: f64,
    pub lock_hold_ms: f64,
}
