//! Manual exact-candidate validation. No receive or landing path is enabled.
use crate::{
    approval::{Approval, ApprovalStore, Decision},
    authority::Authority,
    native_path::NativePath,
    policy::{Binding, TrustedPolicy},
    process::{CancellationToken, ProcessOutcome, RunResult, RunSpec},
    slots::{SlotLease, SlotPool, SlotSettings},
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{fs, path::Path};
#[derive(Debug, Serialize, serde::Deserialize)]
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
#[derive(Debug, Serialize, serde::Deserialize)]
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
    let authority = Authority::open(project)?;
    let policy = TrustedPolicy::load(&authority, candidate)?;
    if policy.config.validators.recovery.as_deref() == Some("supervised-local-v1") {
        return run_supervised_worker(project, candidate, &policy);
    }
    run_local(
        project,
        candidate,
        CancellationToken::default(),
        None,
        false,
    )
}

fn run_local(
    project: &Path,
    candidate: &str,
    cancel: CancellationToken,
    mut handshake: Option<Box<dyn FnOnce() -> Result<()>>>,
    worker: bool,
) -> Result<ValidationResult> {
    let start = std::time::Instant::now();
    let authority = Authority::open(project)?;
    let policy = TrustedPolicy::load(&authority, candidate)?;
    ensure!(
        !cancel.is_cancelled(),
        "validator worker lifeline closed during trusted policy loading"
    );
    if worker {
        ensure!(
            policy.config.validators.recovery.as_deref() == Some("supervised-local-v1"),
            "validator worker requires the trusted supervised-local recovery contract"
        );
    }
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
        if handshake.is_some() {
            let handshake = handshake
                .take()
                .context("validator handshake is unavailable")?;
            handshake()?;
        }
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
                supervised_recovery: settings.recovery.as_deref() == Some("supervised-local-v1"),
            },
        )?;
        let mut lease = pool.acquire_bound(Some(&policy.binding))?;
        if handshake.is_some() {
            let handshake = handshake
                .take()
                .context("validator handshake is unavailable")?;
            if let Err(error) = handshake() {
                let _ = lease.release();
                return Err(error);
            }
        }
        if cancel.is_cancelled() {
            result.outcome = ValidationOutcome::Cancelled;
            result.diagnostic = Some(
                "validator worker lost its engine lifeline before candidate materialization".into(),
            );
        } else {
            lease.materialize_with_cancel(&policy.binding.candidate, &cancel)?;
            let mut bootstrap_failed = false;
            if let Some(command) = &settings.bootstrap {
                let candidate_path = lease.path().to_path_buf();
                let fingerprint = crate::bootstrap::fingerprint_with_runner(
                    &policy,
                    command,
                    &candidate_path,
                    &cancel,
                    |spec, cancellation| lease.run(spec, cancellation),
                )?;
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
                        let mut temp =
                            tempfile::NamedTempFile::new_in(authority.root().join(".mai"))?;
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
            let verified_cancellation = result
                .process
                .as_ref()
                .or(result.bootstrap.as_ref())
                .is_some_and(|process| process.outcome == ProcessOutcome::Cancelled);
            if worker && verified_cancellation && cancel.is_cancelled() {
                if let Err(error) = lease
                    .retain_after_verified_cancellation(format!("worker-{}", std::process::id()))
                {
                    result.outcome = ValidationOutcome::CleanupFailure;
                    result.diagnostic = Some(format!(
                        "could not publish verified cleanup receipt: {error:#}"
                    ));
                }
            } else if let Err(error) = lease.release() {
                result.outcome = ValidationOutcome::CleanupFailure;
                result.diagnostic = Some(format!("{error:#}"));
            }
        }
        // A concurrent trusted-local trunk update invalidates the policy/base result.
        if !cancel.is_cancelled()
            && crate::policy::git(&authority.git_dir(), &["rev-parse", "refs/heads/trunk"])?
                != policy.binding.base
        {
            result.outcome = ValidationOutcome::PolicyChanged;
            result.diagnostic =
                Some("trunk changed during validation; retry against current policy".into());
        }
    }
    result.elapsed_ms = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let cancelled = cancel.is_cancelled() || matches!(result.outcome, ValidationOutcome::Cancelled);
    if !cancelled {
        store.record_result(&result)?;
    }
    Ok(result)
}

/// Child-side worker protocol. The parent owns the authority lock; this child
/// owns the validator slot lock and native process supervision.
pub fn run_worker(project: &Path, candidate: &str) -> Result<ValidationResult> {
    use std::io::{BufRead, BufReader, Read, Write};
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let cancellation = CancellationToken::default();
    let monitor = cancellation.clone();
    let (go_sender, go_receiver) =
        std::sync::mpsc::sync_channel::<std::result::Result<(), String>>(1);
    std::thread::spawn(move || {
        let mut input = BufReader::new(stdin);
        let mut go = String::new();
        let received = input.read_line(&mut go);
        let accepted = match received {
            Ok(_) if go == "GO\n" || go == "GO\r\n" => Ok(()),
            Ok(_) => Err("validator worker did not receive GO".to_owned()),
            Err(error) => Err(format!("read validator worker GO: {error}")),
        };
        let should_watch = accepted.is_ok();
        let _ = go_sender.send(accepted);
        if should_watch {
            let mut sink = [0u8; 1024];
            loop {
                match input.read(&mut sink) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        monitor.cancel();
    });
    let handshake = Box::new(move || -> Result<()> {
        let mut output = stdout.lock();
        writeln!(output, "READY")?;
        output.flush()?;
        go_receiver
            .recv()
            .context("validator engine lifeline closed before GO")?
            .map_err(anyhow::Error::msg)?;
        Ok(())
    });
    let result = run_local(project, candidate, cancellation, Some(handshake), true)?;
    Ok(result)
}

fn read_limited_line(reader: &mut impl std::io::BufRead, limit: usize) -> std::io::Result<String> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            break;
        }
        let length = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if line.len() + length > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "validator worker handshake exceeds limit",
            ));
        }
        let complete = available[length - 1] == b'\n';
        line.extend_from_slice(&available[..length]);
        reader.consume(length);
        if complete {
            break;
        }
    }
    String::from_utf8(line)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn wait_worker_bounded(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            return terminate_worker_bounded(child, timeout);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn terminate_worker_bounded(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::ExitStatus> {
    let _ = child.kill();
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            bail!("validator worker did not exit after termination request");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn worker_lifecycle_bound(policy: &TrustedPolicy) -> std::time::Duration {
    let settings = &policy.config.validators;
    let mut seconds = 300u64 + 240 + 60 * settings.caches.len() as u64;
    seconds = seconds.saturating_add(policy.config.check.timeout_ms.div_ceil(1000));
    let bootstrap = settings.bootstrap.as_ref();
    if let Some(command) = bootstrap {
        seconds = seconds.saturating_add(command.timeout_ms.div_ceil(1000));
        let file_hashes = 1usize
            .saturating_add(command.executable_dependencies.len())
            .saturating_add(command.inputs.len())
            .saturating_add(command.tools.len())
            .saturating_add(1);
        seconds = seconds
            .saturating_add(30 * file_hashes as u64)
            .saturating_add(5 * (command.tools.len() as u64 + 1))
            .saturating_add(30);
    }
    std::time::Duration::from_secs(seconds)
}

fn run_supervised_worker(
    project: &Path,
    candidate: &str,
    policy: &TrustedPolicy,
) -> Result<ValidationResult> {
    use std::io::{BufReader, Read, Write};
    let executable = std::env::current_exe()?;
    ensure!(
        executable.file_stem().is_some_and(|name| name == "mai"),
        "supervised-local recovery requires the mai CLI worker executable; library-only callers cannot start a validator worker"
    );
    const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    const RESPONSE_LIMIT: usize = 32 * 1024 * 1024;
    let worker_timeout = worker_lifecycle_bound(policy);
    let mut child = std::process::Command::new(executable)
        .args(["--json", "validator", "worker"])
        .arg(project)
        .arg(candidate)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;
    let mut child_input = child
        .stdin
        .take()
        .context("missing validator worker lifeline")?;
    let child_output = child
        .stdout
        .take()
        .context("missing validator worker output")?;
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(child_output);
        let result = read_limited_line(&mut reader, 64).map(|line| (reader, line));
        let _ = ack_tx.send(result);
    });
    let (reader, ready) = match ack_rx.recv_timeout(HANDSHAKE_TIMEOUT) {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            drop(child_input);
            let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
            return Err(error.into());
        }
        Err(error) => {
            drop(child_input);
            let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
            return Err(error)
                .context("validator worker did not acknowledge slot ownership in time");
        }
    };
    if ready != "READY\n" {
        drop(child_input);
        let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
        bail!("validator worker failed before slot ownership ACK");
    }
    if let Err(error) = child_input
        .write_all(b"GO\n")
        .and_then(|_| child_input.flush())
    {
        drop(child_input);
        let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
        return Err(error.into());
    }
    let (response_tx, response_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut response = String::new();
        let result = reader
            .take((RESPONSE_LIMIT + 1) as u64)
            .read_to_string(&mut response)
            .map(|_| response);
        let _ = response_tx.send(result);
    });
    let response = match response_rx.recv_timeout(worker_timeout) {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            drop(child_input);
            let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
            return Err(error.into());
        }
        Err(error) => {
            drop(child_input);
            let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
            return Err(error).context("validator worker did not finish in time");
        }
    };
    if response.len() > RESPONSE_LIMIT {
        drop(child_input);
        let _ = terminate_worker_bounded(child, std::time::Duration::from_secs(5));
        bail!("validator worker response exceeds limit");
    }
    drop(child_input);
    let status = wait_worker_bounded(child, std::time::Duration::from_secs(5))?;
    let result: ValidationResult = serde_json::from_str(response.trim())?;
    ensure!(status.success(), "validator worker exited abnormally");
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
