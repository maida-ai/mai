//! Manual exact-candidate validation. No receive or landing path is enabled.
use crate::{
    approval::{Approval, ApprovalStore, Decision},
    authority::Authority,
    native_path::NativePath,
    policy::{Binding, TrustedPolicy},
    process::{CancellationToken, ProcessOutcome, RunResult, RunSpec},
    slots::{SlotLease, SlotPool, SlotSettings},
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{fs, path::Path};
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOutcome {
    Success,
    Failure,
    Timeout,
    Cancelled,
    CleanupFailure,
    ApprovalRequired,
    PolicyDenied,
    PolicyChanged,
}
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResult {
    pub schema_version: u32,
    pub binding: Binding,
    pub outcome: ValidationOutcome,
    pub protected_changes: Vec<NativePath>,
    pub approval: Option<Approval>,
    pub process: Option<RunResult>,
    pub bootstrap: Option<RunResult>,
    pub bootstrap_reused: bool,
    pub elapsed_ms: u64,
    pub diagnostic: Option<String>,
}
/// Caller holds the authority engine ownership lock through the write.
pub(crate) fn approve(
    project: &Path,
    candidate: &str,
    actor: &str,
    reason: &str,
    denied: bool,
) -> Result<Approval> {
    let authority = Authority::open(project)?;
    let policy = TrustedPolicy::load(&authority, candidate)?;
    ApprovalStore::open(authority.root())?.record(
        &policy.binding,
        if denied {
            Decision::Denied
        } else {
            Decision::Approved
        },
        actor,
        reason,
    )
}
pub fn run(project: &Path, candidate: &str) -> Result<ValidationResult> {
    let start = std::time::Instant::now();
    let authority = Authority::open(project)?;
    let policy = TrustedPolicy::load(&authority, candidate)?;
    let mut store = ApprovalStore::open(authority.root())?;
    let approval = store.decision(&policy.binding)?;
    let mut result = ValidationResult {
        schema_version: 1,
        binding: policy.binding.clone(),
        outcome: ValidationOutcome::ApprovalRequired,
        protected_changes: policy.protected_changes.clone(),
        approval: approval.clone(),
        process: None,
        bootstrap: None,
        bootstrap_reused: false,
        elapsed_ms: 0,
        diagnostic: None,
    };
    let denied = approval
        .as_ref()
        .is_some_and(|a| a.decision == Decision::Denied);
    if denied || (!policy.protected_changes.is_empty() && approval.is_none()) {
        if denied {
            result.outcome = ValidationOutcome::PolicyDenied;
        }
    } else {
        let settings = &policy.config.validators;
        let pool = SlotPool::open(
            &authority.root().join(".mai/validators"),
            &authority.git_dir(),
            SlotSettings {
                count: settings.slots,
                port_base: settings.port_base,
                ports_per_slot: settings.ports_per_slot,
                caches: settings.caches.clone(),
            },
        )?;
        let mut lease = pool.acquire()?;
        lease.materialize(&policy.binding.candidate)?;
        let cancel = CancellationToken::default();
        let mut bootstrap_failed = false;
        if let Some(command) = &settings.bootstrap {
            let fingerprint = crate::bootstrap::fingerprint(&policy, command, lease.path())?;
            let stamp = authority
                .root()
                .join(format!(".mai/validator-bootstrap-{}.json", lease.slot()));
            let expected = serde_json::json!({"version":1,"fingerprint":fingerprint});
            let previous = fs::read(&stamp)
                .ok()
                .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok());
            let caches_present = !settings.caches.is_empty()
                && settings
                    .caches
                    .iter()
                    .all(|cache| lease.path().join(cache).is_dir());
            if previous.as_ref() != Some(&expected) || !caches_present {
                // Invalidate before changing reusable environment state, including failed rebuilds.
                match fs::remove_file(&stamp) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                for cache in &settings.caches {
                    let path = lease.path().join(cache);
                    if path.exists() {
                        if fs::symlink_metadata(&path)?.is_dir() {
                            fs::remove_dir_all(path)?;
                        } else {
                            fs::remove_file(path)?;
                        }
                    }
                }
                let spec = policy.command_spec(command, lease.path())?;
                let process = lease.run(&spec, &cancel)?;
                if !matches!(process.outcome, crate::process::ProcessOutcome::Success) {
                    bootstrap_failed = true;
                    result.outcome = outcome(&process);
                    result.bootstrap = Some(process);
                    result.diagnostic =
                        Some("validator bootstrap did not succeed; no gate ran".into());
                } else if !candidate_unchanged(&authority, &policy, &mut lease, &cancel)? {
                    bootstrap_failed = true;
                    result.outcome = ValidationOutcome::Failure;
                    result.bootstrap = Some(process);
                    result.diagnostic = Some(
                        "validator bootstrap modified tracked candidate content; no gate ran"
                            .into(),
                    );
                } else {
                    let mut temp = tempfile::NamedTempFile::new_in(authority.root().join(".mai"))?;
                    use std::io::Write;
                    temp.write_all(&serde_json::to_vec(&expected)?)?;
                    temp.as_file().sync_all()?;
                    temp.persist(&stamp)?;
                    result.bootstrap = Some(process);
                }
            } else {
                result.bootstrap_reused = true;
            }
        }
        if !bootstrap_failed {
            let process = lease.run(
                &policy.command_spec(&policy.config.check, lease.path())?,
                &cancel,
            )?;
            result.outcome = outcome(&process);
            if process.outcome == ProcessOutcome::Success
                && !candidate_unchanged(&authority, &policy, &mut lease, &cancel)?
            {
                result.outcome = ValidationOutcome::Failure;
                result.diagnostic = Some(
                    "gate modified tracked candidate content; original tree was not validated"
                        .into(),
                );
            }
            result.process = Some(process);
        }
        if let Err(error) = lease.release() {
            result.outcome = ValidationOutcome::CleanupFailure;
            result.diagnostic = Some(format!("{error:#}"));
        }
        // A concurrent trusted-local trunk update invalidates the policy/base result.
        if crate::policy::git(&authority.git_dir(), &["rev-parse", "refs/heads/trunk"])?
            != policy.binding.base
        {
            result.outcome = ValidationOutcome::PolicyChanged;
            result.diagnostic =
                Some("trunk changed during validation; retry against current policy".into());
        }
    }
    result.elapsed_ms = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    store.record_result(&result)?;
    Ok(result)
}
/// Compare against trusted objects with a fresh service-owned index. Neither
/// candidate .git/config nor its index/assume-unchanged bits control this check.
fn candidate_unchanged(
    authority: &Authority,
    policy: &TrustedPolicy,
    lease: &mut SlotLease,
    cancel: &CancellationToken,
) -> Result<bool> {
    use crate::repository_scope::external_git_path;
    let temporary = tempfile::tempdir_in(authority.root().join(".mai"))?;
    let mut spec = RunSpec {
        program: "git".into(),
        args: vec![
            "--literal-pathspecs".into(),
            "-c".into(),
            "core.hooksPath=".into(),
            "-c".into(),
            "core.fsmonitor=false".into(),
            "-c".into(),
            "core.bare=false".into(),
            "--git-dir".into(),
            external_git_path(&authority.git_dir()),
            "--work-tree".into(),
            external_git_path(lease.path()),
        ],
        cwd: std::path::PathBuf::from(external_git_path(authority.root())),
        env: [
            (
                "GIT_INDEX_FILE".into(),
                external_git_path(&temporary.path().join("index")),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
            ),
            ("GIT_NO_REPLACE_OBJECTS".into(), "1".into()),
            ("GIT_NO_LAZY_FETCH".into(), "1".into()),
        ]
        .into(),
        timeout: std::time::Duration::from_secs(60),
        output_tail_bytes: 4096,
    };
    let prefix = spec.args.len();
    spec.args
        .extend(["read-tree".into(), policy.binding.tree.clone().into()]);
    let read = lease.run(&spec, cancel)?;
    ensure!(
        read.outcome == ProcessOutcome::Success,
        "cannot prepare candidate integrity check: {:?}",
        read.outcome
    );
    spec.args.truncate(prefix);
    spec.args.extend([
        "diff".into(),
        "--quiet".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        policy.binding.tree.clone().into(),
        "--".into(),
    ]);
    let compared = lease.run(&spec, cancel)?;
    match (&compared.outcome, compared.exit_code) {
        (ProcessOutcome::Success, _) => Ok(true),
        (ProcessOutcome::Failure, Some(1)) => Ok(false),
        _ => anyhow::bail!("cannot confirm candidate integrity: {:?}", compared.outcome),
    }
}

fn outcome(process: &RunResult) -> ValidationOutcome {
    use crate::process::ProcessOutcome::*;
    match process.outcome {
        Success => ValidationOutcome::Success,
        Failure => ValidationOutcome::Failure,
        Timeout => ValidationOutcome::Timeout,
        Cancelled => ValidationOutcome::Cancelled,
        CleanupFailure => ValidationOutcome::Failure,
    }
}
