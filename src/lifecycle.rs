//! Pure, versioned generation state. This module does not register Git refs or certify gate evidence.
//! Callers must durably persist transitions; landing additionally requires the authority's Git/CAS
//! and validation-evidence checks. Restoring a snapshot does not reconstruct missing acknowledgments.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_DIAGNOSTIC_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationId {
    pub change_id: String,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    InvalidSubmission,
    DependencyFailed,
    Conflict,
    CheckFailed,
    PolicyDenied,
    Infrastructure,
}

/// Stable process categories. Pending is not a rejection and transport unavailability is not landing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    InvalidInput = 2,
    Rejected = 3,
    Unavailable = 4,
    Infrastructure = 5,
    Pending = 6,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostics {
    pub conflict: Option<String>,
    pub check: Option<String>,
    pub log_tail: Option<String>,
    pub candidate_ref: Option<String>,
    pub next_action: Option<String>,
}
impl Diagnostics {
    pub fn validate(&self) -> Result<()> {
        for text in [
            &self.conflict,
            &self.check,
            &self.log_tail,
            &self.candidate_ref,
            &self.next_action,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                text.len() <= MAX_DIAGNOSTIC_BYTES && !text.contains('\0'),
                "invalid or oversized diagnostic"
            );
        }
        if let Some(reference) = &self.candidate_ref {
            ensure!(
                reference.starts_with("refs/mai/candidates/")
                    && reference.split('/').all(|part| !part.is_empty()
                        && part
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))),
                "invalid candidate reference"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Landed {
        revision: u64,
        commit: String,
    },
    Rejected {
        reason: ReasonCode,
        diagnostics: Diagnostics,
    },
    Superseded {
        by: GenerationId,
    },
    Cancelled {
        diagnostics: Diagnostics,
    },
}
impl Outcome {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Landed { .. } => ExitCode::Success,
            Self::Rejected {
                reason: ReasonCode::Infrastructure,
                ..
            } => ExitCode::Infrastructure,
            _ => ExitCode::Rejected,
        }
    }
    fn validate(&self) -> Result<()> {
        match self {
            Self::Landed { revision, commit } => {
                ensure!(*revision > 0, "landing cannot use import revision r0");
                oid(commit)?;
            }
            Self::Rejected { diagnostics, .. } | Self::Cancelled { diagnostics } => {
                diagnostics.validate()?
            }
            Self::Superseded { by } => identity(by)?,
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum State {
    Received,
    Queued,
    DependencyBlocked,
    AwaitingApproval,
    Validating { attempt: u64 },
    Terminal { outcome: Outcome },
}
impl State {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal { .. })
    }
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Terminal { outcome } => outcome.exit_code(),
            _ => ExitCode::Pending,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Interrupted,
    Completed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub number: u64,
    pub status: AttemptStatus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Validation,
    Revert,
    Recovery,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub kind: AnnotationKind,
    pub message: String,
}

/// Fields are immutable to callers. Deserialize alone is not admission: only Lifecycle validates and
/// admits snapshots. The database also compares historical immutable fields before saving updates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    id: GenerationId,
    agent: String,
    commit: String,
    dependencies: Vec<GenerationId>,
    predecessor: Option<GenerationId>,
    approval_pending: bool,
    state: State,
    attempts: Vec<Attempt>,
    annotations: Vec<Annotation>,
}
impl Submission {
    pub fn id(&self) -> &GenerationId {
        &self.id
    }
    pub fn agent(&self) -> &str {
        &self.agent
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn dependencies(&self) -> &[GenerationId] {
        &self.dependencies
    }
    pub fn predecessor(&self) -> Option<&GenerationId> {
        self.predecessor.as_ref()
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn attempts(&self) -> &[Attempt] {
        &self.attempts
    }
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }
}

#[derive(Clone, Debug, Default)]
pub struct Lifecycle {
    submissions: BTreeMap<GenerationId, Submission>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    submissions: Vec<Submission>,
}

fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c)),
        "invalid identifier"
    );
    Ok(())
}
fn identity(id: &GenerationId) -> Result<()> {
    name(&id.change_id)?;
    ensure!(id.generation > 0, "generation must be positive");
    Ok(())
}
fn oid(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            && value.bytes().any(|c| c != b'0'),
        "expected nonzero lowercase SHA-1 object id"
    );
    Ok(())
}
impl Lifecycle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, id: &GenerationId) -> Option<&Submission> {
        self.submissions.get(id)
    }
    pub fn submissions(&self) -> impl Iterator<Item = &Submission> {
        self.submissions.values()
    }
    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        Ok(serde_json::to_string(&Snapshot {
            schema_version: SCHEMA_VERSION,
            submissions: self.submissions.values().cloned().collect(),
        })?)
    }
    pub fn from_json(json: &str) -> Result<Self> {
        let snapshot: Snapshot = serde_json::from_str(json)?;
        ensure!(
            snapshot.schema_version == SCHEMA_VERSION,
            "unsupported lifecycle schema"
        );
        let mut lifecycle = Self::new();
        for submission in snapshot.submissions {
            ensure!(
                lifecycle
                    .submissions
                    .insert(submission.id.clone(), submission)
                    .is_none(),
                "duplicate generation"
            );
        }
        lifecycle.validate()?;
        Ok(lifecycle)
    }
    pub fn register(
        &mut self,
        change_id: &str,
        agent: &str,
        commit: &str,
        mut dependencies: Vec<GenerationId>,
    ) -> Result<GenerationId> {
        name(change_id)?;
        name(agent)?;
        oid(commit)?;
        dependencies.sort();
        ensure!(
            dependencies.windows(2).all(|pair| pair[0] != pair[1]),
            "duplicate dependency"
        );
        for parent in &dependencies {
            ensure!(self.get(parent).is_some(), "unknown dependency");
        }
        if let Some(previous) = self.submissions.values().find(|s| s.commit == commit) {
            ensure!(
                previous.id.change_id == change_id
                    && previous.agent == agent
                    && previous.dependencies == dependencies,
                "commit already registered with different immutable metadata"
            );
            return Ok(previous.id.clone());
        }
        let previous = self
            .submissions
            .values()
            .filter(|s| s.id.change_id == change_id)
            .max_by_key(|s| s.id.generation)
            .cloned();
        if let Some(previous) = &previous {
            ensure!(
                previous.agent == agent,
                "change belongs to a different agent"
            );
        }
        let generation = previous.as_ref().map_or(Ok(1), |s| {
            s.id.generation
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("generation exhausted"))
        })?;
        let id = GenerationId {
            change_id: change_id.into(),
            generation,
        };
        let predecessor = previous.as_ref().and_then(|s| {
            if matches!(s.state, State::Validating { .. }) {
                Some(s.id.clone())
            } else {
                s.predecessor
                    .as_ref()
                    .filter(|id| !self.submissions[*id].state.is_terminal())
                    .cloned()
            }
        });
        self.submissions.insert(
            id.clone(),
            Submission {
                id: id.clone(),
                agent: agent.into(),
                commit: commit.into(),
                dependencies,
                predecessor,
                approval_pending: false,
                state: State::Received,
                attempts: vec![],
                annotations: vec![],
            },
        );
        if let Some(previous) = previous {
            if !previous.state.is_terminal() && !matches!(previous.state, State::Validating { .. })
            {
                self.submissions.get_mut(&previous.id).unwrap().state = State::Terminal {
                    outcome: Outcome::Superseded { by: id.clone() },
                };
            }
        }
        self.refresh();
        Ok(id)
    }
    fn mutable(&mut self, id: &GenerationId) -> Result<&mut Submission> {
        let submission = self
            .submissions
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown generation"))?;
        ensure!(
            !submission.state.is_terminal(),
            "terminal outcome is immutable"
        );
        Ok(submission)
    }
    pub fn queue(&mut self, id: &GenerationId) -> Result<()> {
        let s = self.mutable(id)?;
        ensure!(
            matches!(s.state, State::Received),
            "only received generations can enter the queue"
        );
        s.state = State::Queued;
        self.refresh();
        Ok(())
    }
    pub fn require_approval(&mut self, id: &GenerationId) -> Result<()> {
        let s = self.mutable(id)?;
        ensure!(
            matches!(
                s.state,
                State::Received | State::Queued | State::DependencyBlocked
            ),
            "approval cannot interrupt validation"
        );
        s.approval_pending = true;
        s.state = State::AwaitingApproval;
        Ok(())
    }
    pub fn approve(&mut self, id: &GenerationId) -> Result<()> {
        let s = self.mutable(id)?;
        ensure!(
            matches!(s.state, State::AwaitingApproval),
            "generation is not awaiting approval"
        );
        s.approval_pending = false;
        s.state = State::Queued;
        self.refresh();
        Ok(())
    }
    pub fn begin_attempt(&mut self, id: &GenerationId) -> Result<u64> {
        let s = self.mutable(id)?;
        ensure!(
            matches!(s.state, State::Queued),
            "generation is not ready for validation"
        );
        let number = u64::try_from(s.attempts.len())?
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("attempt exhausted"))?;
        s.attempts.push(Attempt {
            number,
            status: AttemptStatus::Running,
        });
        s.state = State::Validating { attempt: number };
        Ok(number)
    }
    fn running(&mut self, id: &GenerationId, attempt: u64) -> Result<&mut Submission> {
        let s = self.mutable(id)?;
        ensure!(
            s.state == State::Validating { attempt },
            "stale or non-running attempt"
        );
        Ok(s)
    }
    pub fn retry_attempt(&mut self, id: &GenerationId, attempt: u64) -> Result<()> {
        let s = self.running(id, attempt)?;
        s.attempts.last_mut().unwrap().status = AttemptStatus::Interrupted;
        s.state = State::Queued;
        self.refresh();
        Ok(())
    }
    pub fn complete_attempt(
        &mut self,
        id: &GenerationId,
        attempt: u64,
        outcome: Outcome,
    ) -> Result<()> {
        outcome.validate()?;
        ensure!(
            matches!(outcome, Outcome::Landed { .. } | Outcome::Rejected { .. }),
            "attempt completion requires landed or rejected outcome"
        );
        let s = self.running(id, attempt)?;
        s.attempts.last_mut().unwrap().status = AttemptStatus::Completed;
        s.state = State::Terminal { outcome };
        self.refresh();
        Ok(())
    }
    pub fn reject(
        &mut self,
        id: &GenerationId,
        reason: ReasonCode,
        diagnostics: Diagnostics,
    ) -> Result<()> {
        diagnostics.validate()?;
        let s = self.mutable(id)?;
        ensure!(
            !matches!(s.state, State::Validating { .. }),
            "use exact attempt identity to reject validation"
        );
        s.state = State::Terminal {
            outcome: Outcome::Rejected {
                reason,
                diagnostics,
            },
        };
        self.refresh();
        Ok(())
    }
    pub fn cancel(&mut self, id: &GenerationId, diagnostics: Diagnostics) -> Result<()> {
        diagnostics.validate()?;
        let s = self.mutable(id)?;
        if matches!(s.state, State::Validating { .. }) {
            s.attempts.last_mut().unwrap().status = AttemptStatus::Completed;
        }
        s.state = State::Terminal {
            outcome: Outcome::Cancelled { diagnostics },
        };
        self.refresh();
        Ok(())
    }
    pub fn annotate(&mut self, id: &GenerationId, annotation: Annotation) -> Result<()> {
        ensure!(
            !annotation.message.is_empty()
                && annotation.message.len() <= MAX_DIAGNOSTIC_BYTES
                && !annotation.message.contains('\0'),
            "invalid annotation"
        );
        let s = self
            .submissions
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown generation"))?;
        ensure!(
            s.state.is_terminal(),
            "annotations describe terminal outcomes"
        );
        s.annotations.push(annotation);
        Ok(())
    }
    fn dependency_failed(&self, s: &Submission) -> bool {
        s.dependencies.iter().any(|id| matches!(self.submissions[id].state, State::Terminal { ref outcome } if !matches!(outcome, Outcome::Landed { .. })))
    }
    fn blocked(&self, s: &Submission) -> bool {
        s.dependencies.iter().any(|id| {
            !matches!(
                self.submissions[id].state,
                State::Terminal {
                    outcome: Outcome::Landed { .. }
                }
            )
        }) || s
            .predecessor
            .as_ref()
            .is_some_and(|id| !self.submissions[id].state.is_terminal())
    }
    fn refresh(&mut self) {
        loop {
            let updates: Vec<_> = self.submissions.values().filter_map(|s| {
                if s.state.is_terminal() { return None; }
                let state = if self.dependency_failed(s) {
                    State::Terminal { outcome: Outcome::Rejected { reason: ReasonCode::DependencyFailed, diagnostics: Diagnostics { next_action: Some("Submit a new generation with explicitly resolved dependencies".into()), ..Diagnostics::default() } } }
                } else if matches!(s.state, State::Queued | State::DependencyBlocked) {
                    if self.blocked(s) { State::DependencyBlocked } else { State::Queued }
                } else { return None; };
                (state != s.state).then_some((s.id.clone(), state))
            }).collect();
            if updates.is_empty() {
                break;
            }
            for (id, state) in updates {
                self.submissions.get_mut(&id).unwrap().state = state;
            }
        }
    }
    /// Validate restored data without repairing it or inferring successful validation.
    pub fn validate(&self) -> Result<()> {
        let mut commits = BTreeSet::new();
        let mut agents = BTreeMap::new();
        let mut generations: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
        for (id, s) in &self.submissions {
            identity(id)?;
            name(&s.agent)?;
            oid(&s.commit)?;
            ensure!(
                id == &s.id && commits.insert(&s.commit),
                "duplicate or mismatched identity"
            );
            if let Some(agent) = agents.insert(&id.change_id, &s.agent) {
                ensure!(agent == &s.agent, "change agent changed");
            }
            generations
                .entry(&id.change_id)
                .or_default()
                .push(id.generation);
            ensure!(
                s.dependencies.windows(2).all(|pair| pair[0] < pair[1]),
                "dependencies are not canonical"
            );
            for parent in &s.dependencies {
                ensure!(
                    self.get(parent).is_some() && parent != id,
                    "unknown or self dependency"
                );
            }
            if let Some(previous) = &s.predecessor {
                ensure!(
                    previous.change_id == id.change_id
                        && previous.generation < id.generation
                        && self.get(previous).is_some(),
                    "invalid predecessor"
                );
            }
            for (index, attempt) in s.attempts.iter().enumerate() {
                ensure!(
                    attempt.number == index as u64 + 1,
                    "nonsequential attempt identity"
                );
                if attempt.status == AttemptStatus::Running {
                    ensure!(
                        index + 1 == s.attempts.len()
                            && s.state
                                == State::Validating {
                                    attempt: attempt.number
                                },
                        "orphan running attempt"
                    );
                }
                if attempt.status == AttemptStatus::Completed {
                    ensure!(
                        index + 1 == s.attempts.len() && s.state.is_terminal(),
                        "completed attempt without terminal outcome"
                    );
                }
            }
            match &s.state {
                State::Validating { attempt } => ensure!(
                    s.attempts.last()
                        == Some(&Attempt {
                            number: *attempt,
                            status: AttemptStatus::Running
                        })
                        && !self.blocked(s)
                        && !s.approval_pending,
                    "invalid active attempt"
                ),
                State::Queued => ensure!(
                    !self.blocked(s) && !s.approval_pending,
                    "queued generation is blocked"
                ),
                State::DependencyBlocked => ensure!(
                    self.blocked(s) && !self.dependency_failed(s) && !s.approval_pending,
                    "invalid dependency-blocked state"
                ),
                State::AwaitingApproval => {
                    ensure!(s.approval_pending, "missing approval requirement")
                }
                State::Received => {
                    ensure!(!s.approval_pending, "unrepresented approval requirement")
                }
                State::Terminal { outcome } => {
                    outcome.validate()?;
                    match outcome {
                        Outcome::Landed { .. } => ensure!(
                            s.attempts
                                .last()
                                .is_some_and(|a| a.status == AttemptStatus::Completed)
                                && !self.blocked(s)
                                && !s.approval_pending,
                            "landing without completed attempt"
                        ),
                        Outcome::Superseded { by } => ensure!(
                            by.change_id == id.change_id
                                && by.generation > id.generation
                                && self.get(by).is_some(),
                            "invalid superseding generation"
                        ),
                        _ => {}
                    }
                }
            }
            ensure!(
                s.state.is_terminal() || !self.dependency_failed(s),
                "unresolved failed dependency"
            );
            ensure!(
                s.annotations.is_empty() || s.state.is_terminal(),
                "nonterminal annotation"
            );
            for annotation in &s.annotations {
                ensure!(
                    !annotation.message.is_empty()
                        && annotation.message.len() <= MAX_DIAGNOSTIC_BYTES
                        && !annotation.message.contains('\0'),
                    "invalid annotation"
                );
            }
        }
        // A restored landing must retain the same predecessor barrier as queued work.
        // Other terminal outcomes can legitimately occur while an older attempt runs.
        for s in self.submissions.values().filter(|s| {
            !s.state.is_terminal()
                || matches!(
                    s.state,
                    State::Terminal {
                        outcome: Outcome::Landed { .. }
                    }
                )
        }) {
            for older in self.submissions.values().filter(|older| {
                older.id.change_id == s.id.change_id
                    && older.id.generation < s.id.generation
                    && !older.state.is_terminal()
            }) {
                ensure!(
                    !older.attempts.is_empty() && s.predecessor.as_ref() == Some(&older.id),
                    "missing earlier-generation validation barrier"
                );
            }
        }
        for values in generations.values() {
            ensure!(
                values.iter().enumerate().all(|(i, n)| *n == i as u64 + 1),
                "generation sequence has gaps"
            );
        }
        for id in self.submissions.keys() {
            self.check_cycle(id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        }
        Ok(())
    }
    fn check_cycle<'a>(
        &'a self,
        id: &'a GenerationId,
        visiting: &mut BTreeSet<&'a GenerationId>,
        checked: &mut BTreeSet<&'a GenerationId>,
    ) -> Result<()> {
        if checked.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            bail!("dependency cycle");
        }
        let s = &self.submissions[id];
        for parent in s.dependencies.iter().chain(s.predecessor.iter()) {
            self.check_cycle(parent, visiting, checked)?;
        }
        visiting.remove(id);
        checked.insert(id);
        Ok(())
    }
}
