//! Explicit serial candidate preparation and gated publication.
//! A prepared tree alone grants no permission to publish; run_one requires the engine writer lock.
use crate::{
    authority::Authority,
    lease::{LeaseBlockReason, LeaseBlocked},
    lifecycle::{GenerationId, Lifecycle, Outcome, State},
    repository_scope::external_git_path,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::process::{Command, Output};

/// Explicit opt-in to Git's textual three-way merge. No mode default is selected here.
#[derive(Clone, Copy, Debug)]
pub enum MergeMode {
    Textual,
}

#[derive(Debug)]
struct MergeConflict(String);
impl std::fmt::Display for MergeConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for MergeConflict {}

#[derive(Debug)]
struct UnsupportedTreePolicy;
impl std::fmt::Display for UnsupportedTreePolicy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            ".gitattributes in a candidate input tree is unsupported for textual merging",
        )
    }
}
impl std::error::Error for UnsupportedTreePolicy {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCandidate {
    pub generation: GenerationId,
    pub submitted: String,
    pub base: String,
    pub trunk: String,
    pub tree: String,
}

/// Apply only the submitted commit's delta. A squashed dependency's submitted commit remains
/// the merge base; its accepted replacement must already be reachable from current trunk.
/// Unsupported merge commits and multiple direct dependencies fail closed.
pub fn prepare_candidate(
    authority: &Authority,
    lifecycle: &Lifecycle,
    id: &GenerationId,
    mode: MergeMode,
) -> Result<PreparedCandidate> {
    lifecycle.validate()?;
    let submission = lifecycle.get(id).context("unknown generation")?;
    ensure!(
        matches!(
            submission.state(),
            State::Queued | State::AwaitingApproval | State::Validating { .. }
        ),
        "generation is not ready for candidate preparation"
    );
    let submitted = submission.commit().to_owned();
    let parents = git(authority, &["show", "-s", "--format=%P", &submitted])?;
    let parents: Vec<_> = parents.split_whitespace().collect();
    ensure!(
        parents.len() == 1,
        "candidate preparation requires one submitted parent"
    );
    let base = parents[0].to_owned();
    let trunk = git(
        authority,
        &["rev-parse", "--verify", "refs/heads/trunk^{commit}"],
    )?;
    match submission.dependencies() {
        [] => ensure!(
            ancestor(authority, &base, &trunk)?,
            "submitted parent is not accepted trunk history"
        ),
        [dependency] => {
            let parent = lifecycle.get(dependency).context("unknown dependency")?;
            ensure!(
                parent.commit() == base,
                "dependency must be the exact submitted parent"
            );
            let State::Terminal {
                outcome: Outcome::Landed { commit, .. },
            } = parent.state()
            else {
                anyhow::bail!("dependency has not landed")
            };
            ensure!(
                ancestor(authority, commit, &trunk)?,
                "mapped dependency is absent from trunk"
            );
        }
        _ => anyhow::bail!("multiple direct dependencies are not supported"),
    }
    match mode {
        MergeMode::Textual => {}
    }
    // Attribute-driven merge behavior is outside this explicit textual-mode contract.
    // Refuse attributes rather than accidentally applying union/binary/custom semantics.
    ensure!(
        !authority.git_dir().join("info/attributes").exists(),
        "repository-local merge attributes are unsupported"
    );
    for tree in [&base, &trunk, &submitted] {
        let paths = command(authority)
            .args(["ls-tree", "-r", "--name-only", "-z", tree])
            .output()?;
        ensure!(
            paths.status.success(),
            "cannot inspect candidate merge attributes"
        );
        if paths
            .stdout
            .split(|b| *b == 0)
            .any(|path| path.rsplit(|b| *b == b'/').next() == Some(b".gitattributes".as_slice()))
        {
            return Err(UnsupportedTreePolicy.into());
        }
    }
    // Never execute repository-configured external merge drivers. They need a trusted-policy
    // integration contract before candidate construction may use them.
    let drivers = command(authority)
        .args(["config", "--get-regexp", r"^merge\..*\.driver$"])
        .output()?;
    ensure!(
        drivers.status.code() == Some(1),
        "custom merge drivers are unsupported or configuration could not be inspected"
    );
    let output = command(authority)
        .args([
            "merge-tree",
            "--write-tree",
            "--name-only",
            "-z",
            &format!("--merge-base={base}"),
            &trunk,
            &submitted,
        ])
        .output()?;
    if output.status.code() == Some(1) {
        return Err(MergeConflict(conflict_diagnostic(&output.stdout)).into());
    }
    ensure!(
        output.status.success(),
        "Git merge-tree failed before producing a clean candidate"
    );
    let text = String::from_utf8(output.stdout)?;
    let tree = text
        .trim_end_matches('\0')
        .lines()
        .next()
        .context("merge-tree omitted tree")?
        .to_owned();
    ensure!(
        tree.len() == 40 && tree.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid merged tree identity"
    );
    Ok(PreparedCandidate {
        generation: id.clone(),
        submitted,
        base,
        trunk,
        tree,
    })
}

fn conflict_diagnostic(output: &[u8]) -> String {
    const MAX_BYTES: usize = 3000;
    const PREFIX: &str = "Submitted changes conflict with current trunk. Affected paths:";
    const OMITTED: &str = "\n- (additional paths omitted)";
    let paths = output
        .iter()
        .position(|byte| *byte == 0)
        .map(|separator| &output[separator + 1..])
        .unwrap_or_default();
    let mut diagnostic = PREFIX.to_owned();
    for path in paths
        .split(|byte| *byte == 0)
        .take_while(|path| !path.is_empty())
    {
        let escaped: String = path
            .iter()
            .copied()
            .flat_map(std::ascii::escape_default)
            .map(char::from)
            .collect();
        let addition = format!("\n- {escaped}");
        if diagnostic.len() + addition.len() > MAX_BYTES {
            if diagnostic.len() + OMITTED.len() <= MAX_BYTES {
                diagnostic.push_str(OMITTED);
            }
            break;
        }
        diagnostic.push_str(&addition);
    }
    if diagnostic == PREFIX {
        diagnostic.push_str("\n- (Git did not report path names)");
    }
    diagnostic
}
fn command(authority: &Authority) -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.arg("--git-dir")
        .arg(external_git_path(&authority.git_dir()))
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "core.fsmonitor=false",
            "-c",
            if cfg!(windows) {
                "core.attributesFile=NUL"
            } else {
                "core.attributesFile=/dev/null"
            },
            "-c",
            "protocol.allow=never",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}
fn git(authority: &Authority, args: &[&str]) -> Result<String> {
    let out = command(authority).args(args).output()?;
    ensure!(out.status.success(), "Git candidate inspection failed");
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}
fn ancestor(authority: &Authority, base: &str, tip: &str) -> Result<bool> {
    let Output { status, .. } = command(authority)
        .args(["merge-base", "--is-ancestor", base, tip])
        .output()?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => anyhow::bail!("Git ancestry inspection failed"),
    }
}

use crate::{
    approval::{ApprovalStore, Decision},
    lease::{LeaseClaim, LeaseStore},
    lifecycle::{Diagnostics, ReasonCode},
    policy::TrustedPolicy,
    state::{StateStore, ValidationEvidence},
    validator::{self, ValidationOutcome},
};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseBlockPhase {
    Preflight,
    PostValidation,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunResult {
    AwaitingApproval {
        candidate: String,
        reference: String,
    },
    Rejected {
        reason: ReasonCode,
    },
    LeaseBlocked {
        unit: String,
        reason: LeaseBlockReason,
        phase: LeaseBlockPhase,
        next_action: &'static str,
    },
    Landed {
        revision: u64,
        commit: String,
    },
}

/// Run one explicitly selected generation while holding the authority engine writer lock.
/// The caller must serialize ALL lifecycle and lease mutations through that same lock.
/// Errors after publication are uncertain outcomes: restart/reconcile before retrying.
/// No priority, automatic queue scheduling or generated-tree mutation is enabled here.
pub fn run_one(
    authority: &mut Authority,
    id: &GenerationId,
    mode: MergeMode,
    leases: &mut LeaseStore,
    claims: &[LeaseClaim],
) -> Result<RunResult> {
    run_one_inner(authority, id, mode, leases, claims, |_| Ok(()))
}

#[derive(Clone, Copy)]
enum Checkpoint {
    EvidenceDurable,
    LeaseBlockBeforeRequeue,
    LeaseBlockAfterRequeue,
    TrunkPublished,
}

fn run_one_inner(
    authority: &mut Authority,
    id: &GenerationId,
    mode: MergeMode,
    leases: &mut LeaseStore,
    claims: &[LeaseClaim],
    mut checkpoint: impl FnMut(Checkpoint) -> Result<()>,
) -> Result<RunResult> {
    run_one_servicing_inner(
        authority,
        id,
        mode,
        leases,
        claims,
        |authority, candidate| validator::run(authority.root(), candidate),
        (&mut checkpoint, |_: &mut Authority, _: &'static str| Ok(())),
    )
}

pub(crate) fn run_one_servicing(
    authority: &mut Authority,
    id: &GenerationId,
    mode: MergeMode,
    leases: &mut LeaseStore,
    claims: &[LeaseClaim],
    validate: impl FnMut(&mut Authority, &str) -> Result<validator::ValidationResult>,
    service: impl FnMut(&mut Authority, &'static str) -> Result<()>,
) -> Result<RunResult> {
    run_one_servicing_inner(
        authority,
        id,
        mode,
        leases,
        claims,
        validate,
        (|_| Ok(()), service),
    )
}

fn run_one_servicing_inner(
    authority: &mut Authority,
    id: &GenerationId,
    mode: MergeMode,
    leases: &mut LeaseStore,
    claims: &[LeaseClaim],
    mut validate: impl FnMut(&mut Authority, &str) -> Result<validator::ValidationResult>,
    callbacks: (
        impl FnMut(Checkpoint) -> Result<()>,
        impl FnMut(&mut Authority, &'static str) -> Result<()>,
    ),
) -> Result<RunResult> {
    let (mut checkpoint, mut service) = callbacks;
    let mut store = StateStore::open(authority.state_path())?;
    let mut life = store.load_lifecycle()?;
    let submission = life.get(id).context("unknown generation")?;
    if let State::Terminal {
        outcome: Outcome::Landed { revision, commit },
    } = submission.state()
    {
        return Ok(RunResult::Landed {
            revision: *revision,
            commit: commit.clone(),
        });
    }
    if let State::Terminal {
        outcome: Outcome::Rejected { reason, .. },
    } = submission.state()
    {
        return Ok(RunResult::Rejected { reason: *reason });
    }
    ensure!(
        matches!(submission.state(), State::Queued | State::AwaitingApproval),
        "generation is not queued; reconcile interrupted attempts before retrying"
    );
    let initial_submission = submission.clone();
    let agent = submission.agent().to_owned();
    let attempt = u64::try_from(submission.attempts().len())?
        .checked_add(1)
        .context("attempt exhausted")?;
    let retained = format!("refs/mai/changes/{}/{}", id.change_id, id.generation);
    ensure!(
        git(authority, &["rev-parse", "--verify", &retained])? == submission.commit(),
        "retained submission reference changed; preserve state and investigate"
    );
    service(authority, "retained identity checked")?;
    life = store.load_lifecycle()?;
    ensure!(
        life.get(id) == Some(&initial_submission),
        "generation changed during preparation"
    );
    // Only immutable Git inputs and a lifecycle snapshot enter the worker. It never
    // writes lifecycle, evidence, approvals or leases; the owner services receipts.
    let root = authority.root().to_path_buf();
    let preparation = std::thread::scope(|scope| {
        let snapshot = &life;
        let worker = scope.spawn(|| {
            let started = std::time::Instant::now();
            let result = (|| {
                let reader = Authority::open(&root)?;
                prepare_candidate(&reader, snapshot, id, mode)
            })();
            eprintln!(
                "candidate preparation {} generation {} attempt {}: worker finished in {}ms (success={})",
                id.change_id, id.generation, attempt, started.elapsed().as_millis(), result.is_ok()
            );
            result
        });
        while !worker.is_finished() {
            service(authority, "preparing candidate")?;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("candidate preparation worker panicked"))?
    });
    service(authority, "candidate prepared")?;
    life = store.load_lifecycle()?;
    ensure!(
        life.get(id) == Some(&initial_submission),
        "generation changed during preparation"
    );
    let prepared = match preparation {
        Ok(candidate) => candidate,
        Err(error) if error.is::<MergeConflict>() => {
            life.reject(id, ReasonCode::Conflict, Diagnostics { conflict: Some(error.to_string()), next_action: Some("Resolve the conflict in your own workspace and submit a new change; dependent changes must be resubmitted against the replacement".into()), ..Diagnostics::default() })?;
            store.save_lifecycle(&mut life)?;
            return Ok(RunResult::Rejected {
                reason: ReasonCode::Conflict,
            });
        }
        Err(error) if error.is::<UnsupportedTreePolicy>() => {
            life.reject(
                id,
                ReasonCode::PolicyDenied,
                Diagnostics {
                    conflict: Some(error.to_string()),
                    next_action: Some("Preserve your workspace. Remove .gitattributes from submitted work and ensure base and trunk also have no attributes; use supported source history and submit corrected work as a new change.".into()),
                    ..Diagnostics::default()
                },
            )?;
            store.save_lifecycle(&mut life)?;
            return Ok(RunResult::Rejected {
                reason: ReasonCode::PolicyDenied,
            });
        }
        Err(error) => return Err(error),
    };
    let records = store.landings()?;
    let revision = if store.baseline()?.as_deref() == Some(&prepared.trunk) {
        ensure!(records.is_empty(), "trunk disagrees with accepted ledger");
        1
    } else {
        let last = records
            .last()
            .context("trunk has no reconciled acceptance record")?;
        ensure!(
            last.commit == prepared.trunk,
            "trunk differs from reconciled acceptance ledger"
        );
        last.revision.checked_add(1).context("revision exhausted")?
    };
    let manifest = crate::manifest::compute(&authority.git_dir(), &prepared.trunk, &prepared.tree)?;
    service(authority, "manifest computed")?;
    life = store.load_lifecycle()?;
    ensure!(
        life.get(id) == Some(&initial_submission),
        "generation changed during preparation"
    );
    let units: Vec<_> = manifest
        .entries
        .iter()
        .map(|entry| entry.unit.canonical())
        .collect();
    if let Err(error) = leases.check_manifest(&agent, &units, claims, now_ms()?) {
        if let Some(blocked) = error.downcast_ref::<LeaseBlocked>() {
            return Ok(lease_blocked_result(blocked, LeaseBlockPhase::Preflight));
        }
        return Err(error);
    }
    let message = format!(
        "Integrate {}\n\nMai-Revision: r{revision}\nMai-Generation: {}\nMai-Attempt: {attempt}\nMai-Policy: {}\nChange-Id: {}\nAgent-ID: {agent}\nMai-Objects: {}\n",
        id.change_id,
        serde_json::to_string(id)?,
        prepared.trunk,
        id.change_id,
        manifest.trailer_value()?
    );
    let output = command(authority)
        .env("GIT_AUTHOR_NAME", &agent)
        .env("GIT_AUTHOR_EMAIL", "agent@mai.local")
        .env("GIT_COMMITTER_NAME", "mai")
        .env("GIT_COMMITTER_EMAIL", "mai@localhost")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit-tree",
            &prepared.tree,
            "-p",
            &prepared.trunk,
            "-m",
            &message,
        ])
        .output()?;
    ensure!(output.status.success(), "candidate commit creation failed");
    let candidate = String::from_utf8(output.stdout)?.trim().to_owned();
    service(authority, "candidate commit created")?;
    life = store.load_lifecycle()?;
    ensure!(
        life.get(id) == Some(&initial_submission),
        "generation changed during preparation"
    );
    let reference = format!(
        "refs/mai/candidates/{}/{}/{attempt}",
        id.change_id, id.generation
    );
    // There is no evidence or running attempt for this next attempt yet. A blocked preflight
    // may be rebuilt against changed trunk; exact approval binding must then be renewed.
    git(authority, &["update-ref", &reference, &candidate])?;
    let policy = TrustedPolicy::load(authority, &candidate)?;
    ensure!(
        policy.binding.base == prepared.trunk && policy.binding.tree == prepared.tree,
        "candidate policy changed during preparation"
    );
    service(authority, "candidate policy loaded")?;
    life = store.load_lifecycle()?;
    ensure!(
        life.get(id) == Some(&initial_submission),
        "generation changed during preparation"
    );
    let approval = ApprovalStore::open(authority.root())?.decision(&policy.binding)?;
    ensure!(
        !approval
            .as_ref()
            .is_some_and(|a| a.decision == Decision::Approved && a.actor == agent),
        "submitted agent cannot approve its own candidate"
    );
    if approval
        .as_ref()
        .is_some_and(|a| a.decision == Decision::Denied)
    {
        life.reject(
            id,
            ReasonCode::PolicyDenied,
            Diagnostics {
                candidate_ref: Some(reference),
                next_action: Some(
                    "Preserve your workspace, resolve the policy denial, and submit a new commit as a new change".into(),
                ),
                ..Diagnostics::default()
            },
        )?;
        store.save_lifecycle(&mut life)?;
        return Ok(RunResult::Rejected {
            reason: ReasonCode::PolicyDenied,
        });
    }
    if !policy.protected_changes.is_empty() && approval.is_none() {
        if matches!(life.get(id).unwrap().state(), State::Queued) {
            life.require_approval(id)?;
            store.save_lifecycle(&mut life)?;
        }
        return Ok(RunResult::AwaitingApproval {
            candidate,
            reference,
        });
    }
    if matches!(life.get(id).unwrap().state(), State::AwaitingApproval) {
        life.approve(id)?;
    }
    ensure!(
        life.begin_attempt(id)? == attempt,
        "attempt changed during preparation"
    );
    store.save_lifecycle(&mut life)?;
    let result = validate(authority, &candidate).context("validation interrupted; restart the engine to reconcile the retained attempt before retrying")?;
    // Receipt and lease operations may have run while validation was in flight.
    // Resume from the current snapshot rather than overwriting those durable writes.
    life = store.load_lifecycle()?;
    ensure!(
        result.binding == policy.binding,
        "validator result binding changed"
    );
    let passed = matches!(result.outcome, ValidationOutcome::Success);
    let evidence = ValidationEvidence {
        generation: id.clone(),
        attempt,
        submitted_commit: prepared.submitted,
        candidate_commit: candidate.clone(),
        candidate_tree: prepared.tree,
        candidate_parent: prepared.trunk.clone(),
        trusted_policy: prepared.trunk.clone(),
        passed,
        manifest: units.clone(),
    };
    store.record_evidence(&evidence)?;
    if !passed {
        let reason = match result.outcome {
            ValidationOutcome::PolicyDenied | ValidationOutcome::ApprovalRequired => {
                ReasonCode::PolicyDenied
            }
            ValidationOutcome::Failure | ValidationOutcome::Timeout => ReasonCode::CheckFailed,
            _ => ReasonCode::Infrastructure,
        };
        let diagnostics = failed_validation_diagnostics(&result, reference)?;
        life.complete_attempt(
            id,
            attempt,
            Outcome::Rejected {
                reason,
                diagnostics,
            },
        )?;
        store.save_lifecycle(&mut life)?;
        return Ok(RunResult::Rejected { reason });
    }
    checkpoint(Checkpoint::EvidenceDurable)?;
    service(authority, "evidence durable; final publication checks")?;
    life = store.load_lifecycle()?;
    ensure!(
        git(authority, &["rev-parse", "refs/heads/trunk"])? == prepared.trunk,
        "trunk changed; restart and reconcile before retrying"
    );
    let fresh = TrustedPolicy::load(authority, &candidate)?;
    ensure!(
        fresh.binding == policy.binding,
        "trusted policy changed before publication"
    );
    let current_approval = ApprovalStore::open(authority.root())?.decision(&fresh.binding)?;
    ensure!(
        !current_approval
            .as_ref()
            .is_some_and(|a| a.decision == Decision::Denied)
            && (fresh.protected_changes.is_empty()
                || current_approval
                    .as_ref()
                    .is_some_and(|a| a.decision == Decision::Approved && a.actor != agent)),
        "approval changed before publication"
    );
    if let Err(error) = leases.check_manifest(&agent, &units, claims, now_ms()?) {
        if let Some(blocked) = error.downcast_ref::<LeaseBlocked>() {
            checkpoint(Checkpoint::LeaseBlockBeforeRequeue)?;
            life.retry_attempt(id, attempt)?;
            store.save_lifecycle(&mut life)?;
            checkpoint(Checkpoint::LeaseBlockAfterRequeue)?;
            return Ok(lease_blocked_result(
                blocked,
                LeaseBlockPhase::PostValidation,
            ));
        }
        return Err(error);
    }
    // No lifecycle, policy or lease writer can interleave here under the engine lock.
    git(
        authority,
        &[
            "update-ref",
            "refs/heads/trunk",
            &candidate,
            &prepared.trunk,
        ],
    )?;
    checkpoint(Checkpoint::TrunkPublished)?;
    drop(store);
    // Publication is already durable. Any servicing or reconciliation failure now
    // leaves an uncertain result that must be queried/reconciled, never retried blindly.
    authority.reconcile_servicing(|authority| service(authority, "reconciling published trunk")).context("trunk publication succeeded but metadata reconciliation failed; preserve state, restart the engine, and query this generation before retrying")?;
    Ok(RunResult::Landed {
        revision,
        commit: candidate,
    })
}

fn lease_blocked_result(blocked: &LeaseBlocked, phase: LeaseBlockPhase) -> RunResult {
    RunResult::LeaseBlocked {
        unit: blocked.unit.clone(),
        reason: blocked.reason,
        phase,
        next_action: "Acquire or renew the matching lease, then explicitly integrate this generation again; validation will run again.",
    }
}

/// Copy already retained validator output without reading workspace files.
fn failed_validation_diagnostics(
    result: &validator::ValidationResult,
    reference: String,
) -> Result<Diagnostics> {
    let outcome = serde_json::to_value(&result.outcome)?;
    let mut check = outcome
        .as_str()
        .context("invalid validator outcome")?
        .to_owned();
    if let Some(detail) = &result.diagnostic {
        check.push_str(": ");
        check.push_str(detail);
    }
    let process = result.process.as_ref().or(result.bootstrap.as_ref());
    if let Some(detail) = process.and_then(|p| p.cleanup_diagnostic.as_ref()) {
        check.push_str(": ");
        check.push_str(detail);
    }
    let log_tail = process.and_then(|process| {
        let budget = (crate::lifecycle::MAX_DIAGNOSTIC_BYTES - 32) / 2;
        let stdout = diagnostic_tail(&process.stdout_tail, budget);
        let stderr = diagnostic_tail(&process.stderr_tail, budget);
        if stdout.is_empty() && stderr.is_empty() {
            None
        } else {
            Some(format!("stdout:\n{stdout}\nstderr:\n{stderr}"))
        }
    });
    // A NUL in native test output must not prevent recording the terminal rejection.
    check = check.replace('\0', "\u{fffd}");
    let mut end = check.len().min(crate::lifecycle::MAX_DIAGNOSTIC_BYTES);
    while !check.is_char_boundary(end) {
        end -= 1;
    }
    check.truncate(end);
    let diagnostics = Diagnostics { check: Some(check), log_tail, candidate_ref: Some(reference), next_action: Some("Preserve your workspace and inspect these gate diagnostics before submitting a corrected commit as a new change".into()), ..Diagnostics::default() };
    diagnostics.validate()?;
    Ok(diagnostics)
}

#[cfg(test)]
mod conflict_diagnostic_tests {
    use super::conflict_diagnostic;

    #[test]
    fn conflict_paths_are_escaped_and_bounded() {
        let mut output = b"tree\0line\nname\0".to_vec();
        output.extend_from_slice(b"bad-\xff-name\0\0ignored source text");
        let diagnostic = conflict_diagnostic(&output);
        assert!(diagnostic.contains("line\\nname"));
        assert!(diagnostic.contains("bad-\\xff-name"));
        assert!(!diagnostic.contains('\0'));
        assert!(!diagnostic.contains("ignored source text"));

        let mut oversized = b"tree\0".to_vec();
        for _ in 0..1000 {
            oversized.extend_from_slice(&[b'x'; 64]);
            oversized.push(0);
        }
        oversized.push(0);
        let diagnostic = conflict_diagnostic(&oversized);
        assert!(diagnostic.len() <= 3000);
        assert!(diagnostic.contains("additional paths omitted"));
    }
}
fn diagnostic_tail(text: &str, budget: usize) -> String {
    let text = text.replace('\0', "\u{fffd}");
    let mut start = text.len().saturating_sub(budget);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

fn now_ms() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

#[cfg(test)]
mod crash_tests {
    use super::*;
    use std::{fs, path::Path};
    #[test]
    fn diagnostic_tails_bound_unicode_and_replace_nuls() {
        let output = format!("{}\0END", "é".repeat(5000));
        let tail = diagnostic_tail(&output, 2000);
        assert!(tail.len() <= 2000);
        assert!(tail.ends_with("\u{fffd}END"));
        assert!(!tail.contains('\0'));
        assert_eq!(diagnostic_tail("", 2000), "");
    }

    // Git for Windows supplies sh, which is already required by the local receive hook.
    // Keep the trusted policy execution path identical across all native CI platforms.
    const POLICY: &str = "version = 1\n[check]\nprogram = \"sh\"\nscript = \"gate.sh\"\ntimeout_ms = 5000\noutput_tail_bytes = 1024\n[validators]\nslots = 1\nport_base = 33000\nports_per_slot = 10\n";
    fn source_git<S: AsRef<std::ffi::OsStr>>(root: &Path, args: &[S]) -> String {
        let mut command = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        let out = command
            .arg("-C")
            .arg(external_git_path(root))
            .args(["-c", "core.autocrlf=false"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .args(args)
            .env("GIT_AUTHOR_NAME", "fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
            .env("GIT_COMMITTER_NAME", "fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    #[test]
    fn crash_child() {
        let Some(root) = std::env::var_os("MAI_TEST_LANDING_ROOT") else {
            return;
        };
        let stage = std::env::var("MAI_TEST_LANDING_STAGE").unwrap();
        let mut a = Authority::open(Path::new(&root)).unwrap();
        let mut leases = a.lease_store().unwrap();
        let id = GenerationId {
            change_id: "change".into(),
            generation: 1,
        };
        let result = run_one_inner(&mut a, &id, MergeMode::Textual, &mut leases, &[], |point| {
            if matches!(
                (stage.as_str(), point),
                ("before", Checkpoint::EvidenceDurable) | ("after", Checkpoint::TrunkPublished)
            ) {
                std::process::exit(42);
            }
            if matches!(
                (stage.as_str(), point),
                ("lease-before", Checkpoint::LeaseBlockBeforeRequeue)
                    | ("lease-after", Checkpoint::LeaseBlockAfterRequeue)
            ) {
                std::process::exit(44);
            }
            if matches!(point, Checkpoint::EvidenceDurable) {
                if stage == "late-lease" || stage.starts_with("lease-") {
                    Authority::open(Path::new(&root))?.lease_store()?.acquire(
                        "file:change",
                        "other",
                        now_ms()?,
                        60_000,
                    )?;
                } else if stage == "stale-trunk" {
                    let authority = Authority::open(Path::new(&root))?;
                    let submitted = git(&authority, &["rev-parse", "refs/mai/changes/change/1"])?;
                    git(&authority, &["update-ref", "refs/heads/trunk", &submitted])?;
                }
            }
            Ok(())
        });
        if stage == "late-lease" {
            assert!(matches!(result, Ok(RunResult::LeaseBlocked { .. })));
            std::process::exit(43);
        }
        if stage == "stale-trunk" {
            assert!(result.is_err());
            std::process::exit(43);
        }
        panic!("expected checkpoint exit: {result:?}");
    }
    #[test]
    fn serviced_receipts_survive_preparation_and_reconciliation_and_late_leases_fence() {
        for late_lease in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let source = tmp.path().join("source");
            fs::create_dir(&source).unwrap();
            source_git(&source, &["init"]);
            fs::write(source.join("gate.sh"), "exit 0\n").unwrap();
            fs::write(source.join("mai.toml"), POLICY).unwrap();
            source_git(&source, &["add", "."]);
            source_git(&source, &["commit", "-m", "policy"]);
            let base = source_git(&source, &["rev-parse", "HEAD"]);
            let mut a = Authority::init(&tmp.path().join("project"), Some(&source)).unwrap();
            crate::receive::enable(&a, 1_000_000, 100_000, 100, true).unwrap();
            fs::write(source.join("change"), "main").unwrap();
            source_git(&source, &["add", "."]);
            source_git(&source, &["commit", "-m", "main"]);
            let submitted = source_git(&source, &["rev-parse", "HEAD"]);
            source_git(&source, &["checkout", "-B", "receipts", &base]);
            let mut commits = Vec::new();
            for index in 0..9 {
                fs::write(source.join("other"), index.to_string()).unwrap();
                source_git(&source, &["add", "."]);
                source_git(&source, &["commit", "-m", "receipt"]);
                commits.push(source_git(&source, &["rev-parse", "HEAD"]));
            }
            for commit in [&submitted, commits.last().unwrap()] {
                source_git(
                    &a.git_dir(),
                    &[
                        std::ffi::OsString::from("fetch"),
                        external_git_path(&source),
                        std::ffi::OsString::from(commit),
                    ],
                );
            }
            let id = crate::receive::register(&a, "main", &submitted)
                .unwrap()
                .id()
                .clone();
            let mut store = StateStore::open(a.state_path()).unwrap();
            let mut life = store.load_lifecycle().unwrap();
            life.queue(&id).unwrap();
            store.save_lifecycle(&mut life).unwrap();
            let mut leases = a.lease_store().unwrap();
            let mut receipts = Vec::new();
            let mut phases = std::collections::BTreeSet::new();
            let mut reconciliation_callbacks = 0;
            let result = run_one_servicing(
                &mut a,
                &id,
                MergeMode::Textual,
                &mut leases,
                &[],
                |authority, candidate| validator::run(authority.root(), candidate),
                |authority, phase| {
                    if phase == "preparing candidate" {
                        return Ok(());
                    }
                    let admit = if phase == "reconciling published trunk" {
                        reconciliation_callbacks += 1;
                        // Include admissions both before and after the reconciliation snapshot.
                        reconciliation_callbacks <= 3
                    } else {
                        phases.insert(phase)
                    };
                    if admit {
                        let receipt =
                            crate::receive::register(authority, "other", &commits[receipts.len()])?;
                        receipts.push(receipt);
                    }
                    if late_lease && phase == "evidence durable; final publication checks" {
                        authority.lease_store()?.acquire(
                            "file:change",
                            "other",
                            now_ms()?,
                            60_000,
                        )?;
                    }
                    Ok(())
                },
            )
            .unwrap();
            let current = store.load_lifecycle().unwrap();
            assert_eq!(receipts.len(), if late_lease { 6 } else { 9 });
            for receipt in &receipts {
                assert_eq!(current.get(receipt.id()), Some(receipt));
            }
            if late_lease {
                assert!(matches!(
                    result,
                    RunResult::LeaseBlocked {
                        phase: LeaseBlockPhase::PostValidation,
                        ..
                    }
                ));
                assert_eq!(current.get(&id).unwrap().state(), &State::Queued);
                assert_eq!(git(&a, &["rev-parse", "trunk"]).unwrap(), base);
                assert!(store.landings().unwrap().is_empty());
            } else {
                assert!(matches!(result, RunResult::Landed { revision: 1, .. }));
                assert_eq!(store.landings().unwrap().len(), 1);
                a.reconcile().unwrap();
                assert_eq!(store.load_lifecycle().unwrap().submissions().count(), 10);
            }
        }
    }

    #[test]
    fn process_interruption_before_and_after_cas_reconciles_without_double_landing() {
        for stage in [
            "before",
            "after",
            "late-lease",
            "stale-trunk",
            "lease-before",
            "lease-after",
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let source = tmp.path().join("source");
            fs::create_dir(&source).unwrap();
            source_git(&source, &["init"]);
            fs::write(source.join("gate.sh"), "exit 0\n").unwrap();
            fs::write(source.join("mai.toml"), POLICY).unwrap();
            source_git(&source, &["add", "."]);
            source_git(&source, &["commit", "-m", "policy"]);
            let mut a = Authority::init(&tmp.path().join("project"), Some(&source)).unwrap();
            let base = git(&a, &["rev-parse", "trunk"]).unwrap();
            fs::write(source.join("change"), "hello").unwrap();
            source_git(&source, &["add", "."]);
            source_git(&source, &["commit", "-m", "change"]);
            let submitted = source_git(&source, &["rev-parse", "HEAD"]);
            source_git(
                &a.git_dir(),
                &[
                    std::ffi::OsString::from("fetch"),
                    external_git_path(&source),
                    std::ffi::OsString::from("HEAD"),
                ],
            );
            let mut store = StateStore::open(a.state_path()).unwrap();
            let mut life = store.load_lifecycle().unwrap();
            let id = life
                .register("change", "agent", &submitted, vec![])
                .unwrap();
            git(&a, &["update-ref", "refs/mai/changes/change/1", &submitted]).unwrap();
            life.queue(&id).unwrap();
            store.save_lifecycle(&mut life).unwrap();
            drop(store);
            let mut leases = a.lease_store().unwrap();
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "landing::crash_tests::crash_child",
                    "--nocapture",
                ])
                .env("MAI_TEST_LANDING_ROOT", a.root())
                .env("MAI_TEST_LANDING_STAGE", stage)
                .status()
                .unwrap();
            assert_eq!(
                status.code(),
                Some(if stage == "before" || stage == "after" {
                    42
                } else if stage.starts_with("lease-") {
                    44
                } else {
                    43
                })
            );
            let interrupted = StateStore::open(a.state_path()).unwrap();
            assert!(interrupted.landings().unwrap().is_empty());
            assert_eq!(interrupted.evidence_all().unwrap().len(), 1);
            let tip = git(&a, &["rev-parse", "trunk"]).unwrap();
            assert_eq!(
                tip == base,
                stage != "stale-trunk" && stage != "after",
                "stage={stage}"
            );
            if stage == "lease-before" {
                let life = interrupted.load_lifecycle().unwrap();
                let submission = life.get(&id).unwrap();
                assert!(matches!(
                    submission.state(),
                    State::Validating { attempt: 1 }
                ));
                assert_eq!(
                    submission.attempts()[0].status,
                    crate::lifecycle::AttemptStatus::Running
                );
            }
            if stage == "lease-after" {
                let life = interrupted.load_lifecycle().unwrap();
                let submission = life.get(&id).unwrap();
                assert_eq!(submission.state(), &State::Queued);
                assert_eq!(
                    submission.attempts()[0].status,
                    crate::lifecycle::AttemptStatus::Interrupted
                );
            }
            if stage == "stale-trunk" {
                assert!(
                    a.reconcile().is_err(),
                    "unqualified foreign trunk mutation must freeze reconciliation"
                );
                git(&a, &["update-ref", "refs/heads/trunk", &base, &tip]).unwrap();
            }
            if stage == "late-lease" || stage.starts_with("lease-") {
                leases
                    .release(
                        "other",
                        &LeaseClaim {
                            unit: "file:change".into(),
                            generation: 1,
                        },
                        now_ms().unwrap(),
                    )
                    .unwrap();
            }
            drop(interrupted);
            a.reconcile().unwrap();
            let result = run_one(&mut a, &id, MergeMode::Textual, &mut leases, &[]).unwrap();
            assert!(matches!(result, RunResult::Landed { revision: 1, .. }));
            a.reconcile().unwrap();
            let store = StateStore::open(a.state_path()).unwrap();
            assert_eq!(store.landings().unwrap().len(), 1);
            let life = store.load_lifecycle().unwrap();
            assert_eq!(
                life.get(&id).unwrap().attempts().len(),
                if stage == "after" { 1 } else { 2 }
            );
        }
    }
}
