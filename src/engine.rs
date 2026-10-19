//! One trusted-local engine owns an authority. The OS lock, never PID metadata,
//! is the ownership decision. Admission, explicit integration, leases and approvals serialize here.
use crate::{
    authority::Authority,
    lifecycle::{GenerationId, Submission},
    native_path::NativePath,
    receive,
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::{Command, Stdio};

const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME: usize = 65536;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const START_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub protocol_version: u32,
    pub pid: u32,
    pub authority: NativePath,
    pub state: String,
    pub registration_available: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol_version: u32,
    action: Action,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Status,
    Stop,
    Ack,
    EnableReceive {
        max_input_bytes: u64,
        max_file_bytes: u64,
        max_changed_files: u64,
        auto_integrate: bool,
    },
    Register {
        agent: String,
        commit: String,
    },
    Show {
        generation: GenerationId,
    },
    ContinuationSnapshot {
        generation: GenerationId,
        suffix: Vec<String>,
    },
    Integrate {
        generation: GenerationId,
        mode: String,
        claims: Vec<crate::lease::LeaseClaim>,
    },
    Lease {
        operation: LeaseAction,
    },
    Approve {
        candidate: String,
        actor: String,
        reason: String,
        denied: bool,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LeaseAction {
    Acquire {
        unit: String,
        owner: String,
        duration_ms: i64,
    },
    Renew {
        owner: String,
        claim: crate::lease::LeaseClaim,
        duration_ms: i64,
    },
    Release {
        owner: String,
        claim: crate::lease::LeaseClaim,
    },
    Query {
        unit: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    protocol_version: u32,
    authority: NativePath,
    result: Option<serde_json::Value>,
    error: Option<String>,
}

fn local_dir(root: &Path) -> Result<PathBuf> {
    ensure!(
        fs::symlink_metadata(root)?.is_dir(),
        "authority must be a real directory, not a symlink"
    );
    let root = root
        .canonicalize()
        .context("authority does not exist; run mai init first")?;
    let local = root.join(".mai");
    ensure!(
        fs::symlink_metadata(&local)?.is_dir(),
        "authority .mai must be a real directory"
    );
    Ok(local)
}
fn regular_or_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => ensure!(
            meta.is_file(),
            "{} must be a regular file, not a symlink",
            path.display()
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn lock_file(local: &Path) -> Result<File> {
    let path = local.join("engine.lock");
    regular_or_absent(&path)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .context("open persistent engine ownership lock")
}

struct Ownership(File);
impl Drop for Ownership {
    fn drop(&mut self) {
        // Explicit unlock also releases a flock inherited briefly across a
        // concurrent fork before CLOEXEC closes that child's descriptor.
        let _ = FileExt::unlock(&self.0);
    }
}

/// Serve until explicit `mai stop`. OS termination releases ownership; a later
/// owner removes stale endpoints only after acquiring this same persistent lock.
pub fn serve(root: &Path) -> Result<()> {
    let local = local_dir(root)?;
    let lock = lock_file(&local)?;
    lock.try_lock_exclusive()
        .context("authority already owned by a live engine; use mai status or mai stop")?;
    let ownership = Ownership(lock);
    let mut authority = Authority::open(root)?;
    authority
        .reconcile()
        .context("authority startup reconciliation failed")?;
    regular_or_absent(&local.join("engine.pid"))?;
    let listener = transport::Listener::bind(&local)?;
    let _cleanup = Cleanup {
        local: local.clone(),
    };
    fs::write(
        local.join("engine.pid"),
        format!("{}\n", std::process::id()),
    )?;
    let mut status = Status {
        protocol_version: PROTOCOL_VERSION,
        pid: std::process::id(),
        authority: NativePath::from_path(&root.canonicalize()?)?,
        state: "ready".into(),
        registration_available: receive::enabled(&authority)?,
    };
    // Durable state is the queue: restart can resume receipts whose replies were lost.
    let mut discovered = std::collections::BTreeSet::new();
    let mut pending = std::collections::VecDeque::new();
    loop {
        if service_request(&listener, &mut authority, &mut status, false)? {
            break;
        }
        if receive::auto_integrate(&authority)? {
            let store = crate::state::StateStore::open(authority.state_path())?;
            let life = store.load_lifecycle()?;
            for submission in life.submissions() {
                if matches!(
                    submission.state(),
                    crate::lifecycle::State::Received | crate::lifecycle::State::Queued
                ) && discovered.insert(submission.id().clone())
                {
                    pending.push_back(submission.id().clone());
                }
            }
            // New receipts join the tail; ready older work cannot be starved.
            let next = pending.iter().position(|id| {
                let s = life.get(id).expect("queued identity remains durable");
                matches!(
                    s.state(),
                    crate::lifecycle::State::Received | crate::lifecycle::State::Queued
                ) && s.dependencies().iter().all(|id| {
                    matches!(
                        life.get(id).map(|s| s.state()),
                        Some(crate::lifecycle::State::Terminal {
                            outcome: crate::lifecycle::Outcome::Landed { .. }
                        })
                    )
                })
            });
            if let Some(index) = next {
                let id = pending.remove(index).unwrap();
                if let Err(error) =
                    integrate_servicing(&mut authority, &id, "textual", &[], &listener, &mut status)
                {
                    // An uncertain attempt must be reconciled on restart, never retried blindly.
                    eprintln!(
                        "automatic integration stopped for {}: {error:#}",
                        id.change_id
                    );
                }
            }
        }
        thread::sleep(POLL);
    }
    drop(listener);
    // Cleanup runs while lock is held, and lock file is never removed.
    drop(_cleanup);
    drop(ownership);
    Ok(())
}
struct Cleanup {
    local: PathBuf,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.local.join("engine.pid"));
        #[cfg(unix)]
        let _ = fs::remove_file(self.local.join("engine.sock"));
    }
}

// The engine remains the sole lifecycle writer, including while a validator runs.
fn service_request(
    listener: &transport::Listener,
    authority: &mut Authority,
    status: &mut Status,
    busy: bool,
) -> Result<bool> {
    if let Some(mut stream) = listener.accept()? {
        let deadline = Instant::now() + IO_TIMEOUT;
        let result = (|| -> Result<bool> {
            let request: Request = read_frame(&mut stream, deadline)?;
            ensure!(
                request.protocol_version == PROTOCOL_VERSION,
                "unsupported IPC version"
            );
            if matches!(
                request.action,
                Action::EnableReceive { .. }
                    | Action::Register { .. }
                    | Action::Show { .. }
                    | Action::ContinuationSnapshot { .. }
                    | Action::Approve { .. }
                    | Action::Integrate { .. }
                    | Action::Lease { .. }
            ) {
                let result: Result<Option<serde_json::Value>> = match request.action {
                    Action::Integrate { .. } | Action::ContinuationSnapshot { .. } if busy => {
                        Err(anyhow::anyhow!(
                            "integration is active; retry after its outcome is recorded"
                        ))
                    }
                    Action::EnableReceive {
                        max_input_bytes,
                        max_file_bytes,
                        max_changed_files,
                        auto_integrate,
                    } => receive::enable(
                        authority,
                        max_input_bytes,
                        max_file_bytes,
                        max_changed_files,
                        auto_integrate,
                    )
                    .map(|()| None),
                    Action::Register { agent, commit } => {
                        receive::register(authority, &agent, &commit)
                            .and_then(|s| Ok(Some(serde_json::to_value(s)?)))
                    }
                    Action::Show { generation } => receive::query(authority, &generation)
                        .and_then(|s| Ok(Some(serde_json::to_value(s)?))),
                    Action::ContinuationSnapshot { generation, suffix } => authority
                        .reconcile()
                        .and_then(|()| continuation_snapshot_owned(authority, &generation, &suffix))
                        .and_then(|s| Ok(Some(serde_json::to_value(s)?))),
                    Action::Integrate {
                        generation,
                        mode,
                        claims,
                    } => integrate_owned_with_validation(
                        authority,
                        &generation,
                        &mode,
                        &claims,
                        |authority, candidate| crate::validator::run(authority.root(), candidate),
                    )
                    .map(Some),
                    Action::Lease { operation } => lease_owned(authority, operation).map(Some),
                    Action::Approve {
                        candidate,
                        actor,
                        reason,
                        denied,
                    } => crate::validator::approve(
                        authority.root(),
                        &candidate,
                        &actor,
                        &reason,
                        denied,
                    )
                    .and_then(|approval| Ok(Some(serde_json::to_value(approval)?))),
                    _ => unreachable!(),
                };
                status.registration_available = receive::enabled(authority)?;
                let mut reply = match result {
                    Ok(result) => Reply {
                        protocol_version: PROTOCOL_VERSION,
                        authority: status.authority.clone(),
                        result,
                        error: None,
                    },
                    Err(error) => Reply {
                        protocol_version: PROTOCOL_VERSION,
                        authority: status.authority.clone(),
                        result: None,
                        error: Some(format!("{error:#}").chars().take(1024).collect()),
                    },
                };
                if serde_json::to_vec(&reply)?.len() > MAX_FRAME {
                    reply.result = None;
                    reply.error = Some("durable result exceeds the bounded IPC response; no result was discarded; inspect authority state".into());
                }
                // An expired/disconnected client changes no durable result.
                let deadline = Instant::now() + IO_TIMEOUT;
                write_frame(&mut stream, &reply, deadline)?;
                let _ = read_frame::<Request>(&mut stream, deadline);
                return Ok(false);
            }
            let stop = match request.action {
                Action::Status => false,
                Action::Stop => !busy,
                Action::Ack => bail!("unexpected acknowledgment"),
                _ => unreachable!(),
            };
            if stop {
                status.state = "stopping".into();
            }
            let written = write_frame(&mut stream, &status, deadline);
            // Once a valid stop is accepted, a disconnected requester must
            // not leave a live engine advertising a false stopping state.
            if stop {
                if written.is_ok() {
                    let _ = read_frame::<Request>(&mut stream, deadline);
                }
                return Ok(true);
            }
            written?;
            // A bounded acknowledgment guarantees Windows clients consumed
            // the response without an unbounded FlushFileBuffers call.
            if let Ok(ack) = read_frame::<Request>(&mut stream, deadline) {
                ensure!(
                    ack.protocol_version == PROTOCOL_VERSION && matches!(ack.action, Action::Ack),
                    "invalid acknowledgment"
                );
            }
            Ok(stop)
        })();
        match result {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(error) => eprintln!("local IPC request rejected: {error:#}"),
        }
    }
    Ok(false)
}

fn integrate_servicing(
    authority: &mut Authority,
    id: &GenerationId,
    mode: &str,
    claims: &[crate::lease::LeaseClaim],
    listener: &transport::Listener,
    status: &mut Status,
) -> Result<serde_json::Value> {
    integrate_owned_with_validation(authority, id, mode, claims, |authority, candidate| {
        let root = authority.root().to_path_buf();
        let candidate = candidate.to_owned();
        thread::scope(|scope| {
            let worker = scope.spawn(|| crate::validator::run(&root, &candidate));
            while !worker.is_finished() {
                service_request(listener, authority, status, true)?;
                thread::sleep(POLL);
            }
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("validator worker panicked"))?
        })
    })
}

fn exchange(local: &Path, action: Action, timeout: Duration) -> Result<Status> {
    let deadline = Instant::now() + timeout;
    let mut stream = transport::connect(local, deadline)?;
    write_frame(
        &mut stream,
        &Request {
            protocol_version: PROTOCOL_VERSION,
            action,
        },
        deadline,
    )?;
    let response: Status = read_frame(&mut stream, deadline)?;
    ensure!(
        response.protocol_version == PROTOCOL_VERSION,
        "unsupported engine IPC version"
    );
    ensure!(
        response.authority.as_path() == local.parent().unwrap(),
        "engine responded for another authority"
    );
    write_frame(
        &mut stream,
        &Request {
            protocol_version: PROTOCOL_VERSION,
            action: Action::Ack,
        },
        deadline,
    )?;
    Ok(response)
}

pub fn status(root: &Path, autostart: bool) -> Result<Status> {
    let local = local_dir(root)?;
    if let Ok(status) = exchange(&local, Action::Status, Duration::from_millis(200)) {
        return Ok(status);
    }
    if !autostart {
        bail!(
            "engine unavailable; run mai serve {} --foreground or mai status {} to autostart",
            root.display(),
            root.display()
        );
    }
    let deadline = Instant::now() + START_TIMEOUT;
    let log_path = local.join("engine.log");
    regular_or_absent(&log_path)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let mut child = spawn_background(&root.canonicalize()?, log)?;
    loop {
        if let Ok(status) = exchange(&local, Action::Status, Duration::from_millis(200)) {
            return Ok(status);
        }
        if let Some(exit) = child.try_wait()? {
            // Another racing starter may hold ownership and still reconcile.
            let lock = lock_file(&local)?;
            if lock.try_lock_exclusive().is_ok() {
                FileExt::unlock(&lock)?;
                bail!(
                    "engine exited ({exit}) before readiness; inspect {} and run mai serve --foreground",
                    log_path.display()
                );
            }
        }
        ensure!(
            Instant::now() < deadline,
            "engine readiness timed out; inspect {} and run mai serve --foreground (a starting/live owner is never killed)",
            log_path.display()
        );
        thread::sleep(POLL);
    }
}

#[cfg(unix)]
fn spawn_background(root: &Path, log: File) -> Result<std::process::Child> {
    Command::new(std::env::current_exe()?)
        .arg("serve")
        .arg(root)
        .arg("--foreground")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .context("failed to spawn engine")
}

#[cfg(windows)]
use background::spawn_background;

#[cfg(windows)]
#[path = "engine/windows_process.rs"]
mod background;

/// An identity-checked engine reply, distinct from a missing transport response.
#[derive(Debug)]
struct EngineOperationError(String);
impl std::fmt::Display for EngineOperationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for EngineOperationError {}

fn operation(root: &Path, action: Action) -> Result<Option<serde_json::Value>> {
    operation_with_timeout(root, action, Duration::from_secs(10))
}
fn operation_with_timeout(
    root: &Path,
    action: Action,
    timeout: Duration,
) -> Result<Option<serde_json::Value>> {
    let local = local_dir(root)?;
    let deadline = Instant::now() + timeout;
    let mut stream = transport::connect(&local, deadline)?;
    write_frame(
        &mut stream,
        &Request {
            protocol_version: PROTOCOL_VERSION,
            action,
        },
        deadline,
    )?;
    let response: Reply = read_frame(&mut stream, deadline)?;
    ensure!(
        response.protocol_version == PROTOCOL_VERSION
            && response.authority.as_path() == local.parent().unwrap(),
        "engine IPC identity mismatch"
    );
    // The response is already received and its identity verified. Failure to
    // acknowledge it cannot turn that known result into a lost response.
    let _ = write_frame(
        &mut stream,
        &Request {
            protocol_version: PROTOCOL_VERSION,
            action: Action::Ack,
        },
        deadline,
    );
    if let Some(error) = response.error {
        return Err(EngineOperationError(error).into());
    }
    Ok(response.result)
}
pub fn enable_receive(
    root: &Path,
    max_input_bytes: u64,
    max_file_bytes: u64,
    max_changed_files: u64,
    auto_integrate: bool,
) -> Result<()> {
    status(root, true)?;
    operation(
        root,
        Action::EnableReceive {
            max_input_bytes,
            max_file_bytes,
            max_changed_files,
            auto_integrate,
        },
    )?;
    Ok(())
}
pub fn register(root: &Path, agent: &str, commit: &str) -> Result<Submission> {
    Ok(serde_json::from_value(
        operation(
            root,
            Action::Register {
                agent: agent.into(),
                commit: commit.into(),
            },
        )?
        .context("engine returned no submission")?,
    )?)
}
pub fn show(root: &Path, generation: GenerationId) -> Result<Submission> {
    let lifecycle = observer_lifecycle(root)?;
    let submission = lifecycle
        .get(&generation)
        .context("unknown submission generation")?;
    let mut response = serde_json::to_value(submission)?;
    response["schema_version"] = serde_json::json!(crate::lifecycle::SCHEMA_VERSION);
    ensure!(
        serde_json::to_vec(&response)?.len() <= MAX_FRAME,
        "durable result exceeds the bounded show response; inspect authority state"
    );
    Ok(submission.clone())
}
pub fn queue(root: &Path, limit: usize, after: Option<GenerationId>) -> Result<serde_json::Value> {
    ensure!(
        (1..=100).contains(&limit),
        "queue limit must be between 1 and 100"
    );
    let lifecycle = observer_lifecycle(root)?;
    let all: Vec<_> = lifecycle.submissions().collect();
    let start = match after {
        Some(cursor) => {
            all.iter()
                .position(|s| s.id() == &cursor)
                .context("queue cursor generation not found")?
                + 1
        }
        None => 0,
    };
    let end = start.saturating_add(limit).min(all.len());
    let items: Vec<_> = all[start..end]
        .iter()
        .map(|submission| {
            let dependencies = submission.dependencies();
            let state = serde_json::to_value(submission.state())?;
            Ok(serde_json::json!({
                "id": submission.id(),
                "agent": submission.agent(),
                "commit": submission.commit(),
                "dependencies": dependencies.iter().take(8).collect::<Vec<_>>(),
                "dependency_count": dependencies.len(),
                "dependencies_truncated": dependencies.len() > 8,
                "state": state.get("kind").cloned().unwrap_or(serde_json::Value::Null),
                "outcome": state.get("outcome").map(|outcome| serde_json::json!({
                    "kind": outcome.get("kind"),
                    "reason": outcome.get("reason")
                })),
                "attempt_count": submission.attempts().len(),
                "annotation_count": submission.annotations().len()
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let next_cursor = if end < all.len() {
        all.get(end - 1).map(|submission| submission.id().clone())
    } else {
        None
    };
    let result = serde_json::json!({"items":items,"next_cursor":next_cursor,"total":all.len(),"order":"change_id_generation"});
    ensure!(
        serde_json::to_vec(&result)?.len() <= 255 * 1024,
        "queue page exceeds the 256 KiB response bound; retry with a smaller --limit"
    );
    Ok(result)
}
pub fn wait(root: &Path, generation: GenerationId, timeout: Duration) -> Result<serde_json::Value> {
    ensure!(
        (Duration::from_secs(1)..=Duration::from_secs(3600)).contains(&timeout),
        "timeout must be between 1 and 3600 seconds"
    );
    let deadline = Instant::now() + timeout;
    let mut last_state = None;
    let mut observed_identity = None;
    loop {
        if Instant::now() >= deadline {
            return pending_wait(&generation, last_state);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (lifecycle, authority_id, baseline) = match observer_snapshot(root, remaining) {
            Ok(snapshot) => snapshot,
            Err(error) if Instant::now() >= deadline && is_sqlite_lock_timeout(&error) => {
                return pending_wait(&generation, last_state);
            }
            Err(error) => return Err(error),
        };
        if let Some((expected_id, expected_baseline)) = &observed_identity {
            ensure!(
                authority_id == *expected_id && baseline == *expected_baseline,
                "authority identity changed while waiting; retry against the intended authority"
            );
        } else {
            observed_identity = Some((authority_id.clone(), baseline.clone()));
        }
        let submission = lifecycle
            .get(&generation)
            .context("unknown submission generation")?;
        let state = submission.state().clone();
        if Instant::now() >= deadline {
            return pending_wait(&generation, last_state);
        }
        if submission.state().is_terminal() {
            let result = bounded_wait_result(submission)?;
            ensure!(
                serde_json::to_vec(&result)?.len() <= 31 * 1024,
                "terminal wait record exceeds the 32 KiB result bound; inspect the exact generation with mai show"
            );
            return Ok(result);
        }
        last_state = Some(state);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return pending_wait(&generation, last_state);
        }
        thread::sleep(remaining.min(Duration::from_millis(200)));
    }
}
fn pending_wait(
    generation: &GenerationId,
    state: Option<crate::lifecycle::State>,
) -> Result<serde_json::Value> {
    let result = serde_json::json!({"kind":"pending","generation":generation,"state":state,"message":"wait timed out before a terminal outcome; submission state is unchanged"});
    ensure!(
        serde_json::to_vec(&result)?.len() <= 32 * 1024,
        "pending wait result exceeds the response bound"
    );
    Ok(result)
}
fn is_sqlite_lock_timeout(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<rusqlite::Error>()
            .is_some_and(|error| match error {
                rusqlite::Error::SqliteFailure(error, _) => matches!(
                    error.code,
                    rusqlite::ffi::ErrorCode::DatabaseBusy
                        | rusqlite::ffi::ErrorCode::DatabaseLocked
                ),
                _ => false,
            })
    })
}
fn bounded_wait_result(submission: &Submission) -> Result<serde_json::Value> {
    let dependencies = submission.dependencies();
    Ok(serde_json::json!({
        "id": submission.id(),
        "agent": submission.agent(),
        "commit": submission.commit(),
        "dependencies": dependencies.iter().take(8).collect::<Vec<_>>(),
        "dependency_count": dependencies.len(),
        "dependencies_truncated": dependencies.len() > 8,
        "state": submission.state(),
        "attempt_count": submission.attempts().len(),
        "annotation_count": submission.annotations().len(),
        "history_omitted": true
    }))
}
fn observer_snapshot(
    root: &Path,
    timeout: Duration,
) -> Result<(crate::lifecycle::Lifecycle, String, String)> {
    let local = local_dir(root)?;
    let config_path = local.join("config.toml");
    ensure!(
        fs::symlink_metadata(&config_path)?.is_file(),
        "authority configuration must be a regular file"
    );
    let config: toml::Value = toml::from_str(&fs::read_to_string(config_path)?)?;
    ensure!(
        config
            .get("format_version")
            .and_then(toml::Value::as_integer)
            == Some(2),
        "unsupported authority format"
    );
    let authority_id = config
        .get("authority_id")
        .and_then(toml::Value::as_str)
        .context("authority configuration has no identity")?;
    let baseline = config
        .get("baseline")
        .and_then(toml::Value::as_str)
        .context("authority configuration has no import baseline")?;
    let path = local.join("state.db");
    ensure!(
        fs::symlink_metadata(&path)?.is_file(),
        "authority state must be a regular file"
    );
    let (lifecycle, stored_id, stored_baseline) =
        crate::state::StateStore::observe_lifecycle_with_timeout(path, timeout)?;
    let stored_id = stored_id.context("read-only state has no authority identity")?;
    let stored_baseline = stored_baseline.context("read-only state has no import baseline")?;
    ensure!(
        stored_id == authority_id,
        "authority identity does not match read-only state"
    );
    ensure!(
        stored_baseline == baseline,
        "authority baseline does not match read-only state"
    );
    Ok((lifecycle, stored_id, stored_baseline))
}
fn observer_lifecycle(root: &Path) -> Result<crate::lifecycle::Lifecycle> {
    Ok(observer_snapshot(root, Duration::from_secs(5))?.0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationSnapshot {
    pub schema_version: u32,
    pub generation: GenerationId,
    pub agent: String,
    pub submitted_commit: String,
    pub landed_commit: String,
    pub revision: u64,
    pub base_trunk: String,
}

fn continuation_snapshot_owned(
    authority: &Authority,
    generation: &GenerationId,
    suffix: &[String],
) -> Result<ContinuationSnapshot> {
    ensure!(
        suffix.len() <= 256,
        "continuation suffix exceeds 256 commits"
    );
    for commit in suffix {
        ensure!(
            commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid suffix commit id"
        );
    }
    ensure!(
        suffix
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == suffix.len(),
        "duplicate suffix commit"
    );
    let store = crate::state::StateStore::open(authority.state_path())?;
    let lifecycle = store.load_lifecycle()?;
    let submission = lifecycle
        .get(generation)
        .context("unknown continuation generation")?;
    let (revision, landed_commit) = match submission.state() {
        crate::lifecycle::State::Terminal {
            outcome: crate::lifecycle::Outcome::Landed { revision, commit },
        } => (*revision, commit.clone()),
        _ => anyhow::bail!("selected generation is not landed"),
    };
    let landing = store
        .landings()?
        .into_iter()
        .find(|landing| &landing.generation == generation)
        .context("landed generation has no immutable landing record")?;
    ensure!(
        landing.revision == revision && landing.commit == landed_commit,
        "landing record disagrees with terminal outcome"
    );
    let base_trunk = authority
        .git(&["rev-parse", "refs/heads/trunk^{commit}"], None)?
        .trim()
        .to_owned();
    ensure!(
        authority
            .git(&["rev-list", "--first-parent", &base_trunk], None)?
            .lines()
            .any(|line| line == landed_commit),
        "recorded landed commit is not on trunk's first-parent chain"
    );
    let registered: std::collections::BTreeSet<_> = lifecycle
        .submissions()
        .map(|item| item.commit().to_owned())
        .collect();
    for commit in suffix {
        ensure!(
            !registered.contains(commit),
            "suffix contains registered generation {commit}; select a clean unregistered suffix"
        );
    }
    Ok(ContinuationSnapshot {
        schema_version: 1,
        generation: generation.clone(),
        agent: submission.agent().to_owned(),
        submitted_commit: submission.commit().to_owned(),
        landed_commit,
        revision,
        base_trunk,
    })
}

pub fn continuation_snapshot(
    root: &Path,
    generation: GenerationId,
    suffix: Vec<String>,
) -> Result<ContinuationSnapshot> {
    ensure!(
        suffix.len() <= 256,
        "continuation suffix exceeds 256 commits"
    );
    status(root, true)?;
    Ok(serde_json::from_value(
        operation(root, Action::ContinuationSnapshot { generation, suffix })?
            .context("engine returned no continuation snapshot")?,
    )?)
}

fn lease_store(authority: &Authority) -> Result<crate::lease::LeaseStore> {
    authority.lease_store()
}

fn lease_owned(authority: &Authority, action: LeaseAction) -> Result<serde_json::Value> {
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    let mut store = lease_store(authority)?;
    let result = match action {
        LeaseAction::Acquire {
            unit,
            owner,
            duration_ms,
        } => serde_json::to_value(store.acquire(&unit, &owner, now, duration_ms)?)?,
        LeaseAction::Renew {
            owner,
            claim,
            duration_ms,
        } => serde_json::to_value(store.renew(&owner, &claim, now, duration_ms)?)?,
        LeaseAction::Release { owner, claim } => {
            store.release(&owner, &claim, now)?;
            serde_json::json!({"released":true,"claim":claim})
        }
        LeaseAction::Query { unit } => serde_json::to_value(store.query(&unit, now)?)?,
    };
    Ok(serde_json::json!({"schema_version":1,"lease":result}))
}
fn integrate_owned_with_validation(
    authority: &mut Authority,
    id: &GenerationId,
    mode: &str,
    claims: &[crate::lease::LeaseClaim],
    validate: impl FnMut(&mut Authority, &str) -> Result<crate::validator::ValidationResult>,
) -> Result<serde_json::Value> {
    ensure!(
        mode == "textual",
        "only explicitly selected textual merge mode is available"
    );
    ensure!(claims.len() <= 256, "too many lease claims");
    let mut store = crate::state::StateStore::open(authority.state_path())?;
    let mut lifecycle = store.load_lifecycle()?;
    let state = lifecycle
        .get(id)
        .context("unknown submission generation")?
        .state();
    if matches!(state, crate::lifecycle::State::Received) {
        lifecycle.queue(id)?;
        store.save_lifecycle(&mut lifecycle)?;
    }
    ensure!(
        !matches!(
            lifecycle.get(id).unwrap().state(),
            crate::lifecycle::State::DependencyBlocked
        ),
        "dependency blocked; integrate the submitted parent before retrying this change"
    );
    let mut leases = lease_store(authority)?;
    let mut result = serde_json::to_value(crate::landing::run_one_servicing(
        authority,
        id,
        crate::landing::MergeMode::Textual,
        &mut leases,
        claims,
        validate,
    )?)?;
    result["schema_version"] = serde_json::json!(1);
    result["generation"] = serde_json::to_value(id)?;
    Ok(result)
}
pub fn integrate(
    root: &Path,
    generation: GenerationId,
    mode: String,
    claims: Vec<crate::lease::LeaseClaim>,
    wait_seconds: u64,
) -> Result<serde_json::Value> {
    ensure!(
        (1..=3600).contains(&wait_seconds),
        "wait-seconds must be between 1 and 3600"
    );
    operation_with_timeout(root, Action::Integrate { generation, mode, claims }, Duration::from_secs(wait_seconds))
        .map_err(|error| {
            if error.is::<EngineOperationError>() {
                error
            } else {
                error.context("integration response unavailable; outcome may be uncertain; query the same generation before retrying")
            }
        })?.context("engine returned no integration result")
}
pub fn lease(root: &Path, operation: LeaseAction) -> Result<serde_json::Value> {
    operation_with_timeout(root, Action::Lease { operation }, Duration::from_secs(10))?
        .context("engine returned no lease result")
}

fn ownership_contended(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
        || error
            .raw_os_error()
            .is_some_and(|code| Some(code) == fs2::lock_contended_error().raw_os_error())
}

/// Administration shares publication ownership, including when no daemon runs.
/// A failed IPC response is uncertain and is never retried as an unlocked write.
pub fn approve(
    root: &Path,
    candidate: &str,
    actor: &str,
    reason: &str,
    denied: bool,
) -> Result<crate::approval::Approval> {
    let local = local_dir(root)?;
    let lock = lock_file(&local)?;
    match lock.try_lock_exclusive() {
        Ok(()) => {
            let _ownership = Ownership(lock);
            crate::validator::approve(root, candidate, actor, reason, denied)
        }
        Err(error) if ownership_contended(&error) => Ok(serde_json::from_value(
            operation(
                root,
                Action::Approve {
                    candidate: candidate.into(),
                    actor: actor.into(),
                    reason: reason.into(),
                    denied,
                },
            )?
            .context("engine returned no approval")?,
        )?),
        Err(error) => Err(error).context("acquire authority ownership for approval"),
    }
}

/// Inspect one validator claim while holding publication ownership.
pub fn validator_inspect(root: &Path, slot: usize) -> Result<crate::slots::SlotInspection> {
    let local = local_dir(root)?;
    let lock = lock_file(&local)?;
    lock.try_lock_exclusive()
        .context("authority is active; stop it before inspecting validator state")?;
    let _ownership = Ownership(lock);
    let mut authority = Authority::open(root)?;
    authority.reconcile()?;
    let policy = crate::policy::TrustedPolicy::load(&authority, "refs/heads/trunk")?;
    let settings = &policy.config.validators;
    let pool = crate::slots::SlotPool::open(
        &authority.root().join(".mai/validators"),
        &authority.git_dir(),
        crate::slots::SlotSettings {
            count: settings.slots,
            port_base: settings.port_base,
            ports_per_slot: settings.ports_per_slot,
            caches: settings.caches.clone(),
            supervised_recovery: settings.recovery.as_deref() == Some("supervised-local-v1"),
        },
    )?;
    pool.inspect(slot)
}

/// Explicit cleanup-only recovery; validation and landing require a fresh attempt.
pub fn validator_recover(root: &Path, slot: usize, claim_id: &str) -> Result<std::path::PathBuf> {
    let local = local_dir(root)?;
    let lock = lock_file(&local)?;
    lock.try_lock_exclusive()
        .context("authority is active; stop it before validator recovery")?;
    let _ownership = Ownership(lock);
    let mut authority = Authority::open(root)?;
    authority.reconcile()?;
    let policy = crate::policy::TrustedPolicy::load(&authority, "refs/heads/trunk")?;
    let settings = &policy.config.validators;
    anyhow::ensure!(
        settings.recovery.as_deref() == Some("supervised-local-v1"),
        "current trusted trunk policy does not enable supervised-local recovery"
    );
    let pool = crate::slots::SlotPool::open(
        &authority.root().join(".mai/validators"),
        &authority.git_dir(),
        crate::slots::SlotSettings {
            count: settings.slots,
            port_base: settings.port_base,
            ports_per_slot: settings.ports_per_slot,
            caches: settings.caches.clone(),
            supervised_recovery: true,
        },
    )?;
    pool.recover(slot, claim_id)
}

pub fn stop(root: &Path) -> Result<Status> {
    let status = exchange(&local_dir(root)?, Action::Stop, IO_TIMEOUT)
        .context("engine unavailable; stop never autostarts an engine")?;
    ensure!(
        status.state == "stopping",
        "integration is active; retry stop after its outcome is recorded"
    );
    Ok(status)
}

fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut impl Read,
    deadline: Instant,
) -> Result<T> {
    let mut bytes = Vec::new();
    loop {
        ensure!(Instant::now() < deadline, "IPC read timed out");
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => bail!("IPC peer closed before a complete frame"),
            Ok(_) if byte[0] == b'\n' => {
                return serde_json::from_slice(&bytes).context("invalid IPC frame");
            }
            Ok(_) => {
                ensure!(
                    bytes.len() < MAX_FRAME,
                    "IPC frame exceeds {MAX_FRAME} bytes"
                );
                bytes.push(byte[0]);
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::Interrupted =>
            {
                thread::sleep(POLL)
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn write_frame(stream: &mut impl Write, value: &impl Serialize, deadline: Instant) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "IPC frame too large");
    bytes.push(b'\n');
    let mut remaining = bytes.as_slice();
    while !remaining.is_empty() {
        ensure!(Instant::now() < deadline, "IPC write timed out");
        match stream.write(remaining) {
            Ok(0) => bail!("IPC peer stopped accepting data"),
            Ok(n) => remaining = &remaining[n..],
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::Interrupted =>
            {
                thread::sleep(POLL)
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    };
    pub struct Listener(UnixListener);
    impl Listener {
        pub fn bind(local: &Path) -> Result<Self> {
            let path = local.join("engine.sock");
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(
                    meta.file_type().is_socket(),
                    "refusing to replace non-socket engine.sock"
                );
                fs::remove_file(&path)?;
            }
            let listener = UnixListener::bind(&path)
                .context("bind engine socket (authority path may exceed OS socket path limit)")?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            Ok(Self(listener))
        }
        pub fn accept(&self) -> Result<Option<UnixStream>> {
            match self.0.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    Ok(Some(stream))
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
    }
    pub fn connect(local: &Path, deadline: Instant) -> Result<UnixStream> {
        use std::os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        };
        let path = local.join("engine.sock");
        let bytes = path.as_os_str().as_bytes();
        // SAFETY: zero is a valid initial sockaddr_un; fields are filled below.
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        ensure!(
            bytes.len() < address.sun_path.len() && !bytes.contains(&0),
            "authority path exceeds Unix socket path limit"
        );
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (out, byte) in address.sun_path.iter_mut().zip(bytes) {
            *out = *byte as libc::c_char;
        }
        let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1;
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        {
            address.sun_len = length as u8;
        }
        loop {
            ensure!(Instant::now() < deadline, "IPC connect timed out");
            // SAFETY: socket has no pointer arguments and returns a new owned fd.
            let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
            ensure!(
                raw >= 0,
                "create Unix socket: {}",
                io::Error::last_os_error()
            );
            // SAFETY: raw is a fresh successful socket and ownership transfers once.
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: fcntl operates on a valid owned socket. Set nonblocking
            // before connect so a full listener backlog cannot hang the CLI.
            ensure!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } >= 0,
                "set socket nonblocking: {}",
                io::Error::last_os_error()
            );
            ensure!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } >= 0,
                "set socket close-on-exec: {}",
                io::Error::last_os_error()
            );
            // SAFETY: initialized sockaddr_un contains a NUL-terminated path;
            // length describes its initialized family/path bytes on this OS.
            let connected = unsafe {
                libc::connect(
                    fd.as_raw_fd(),
                    (&address as *const libc::sockaddr_un).cast(),
                    length as libc::socklen_t,
                )
            };
            if connected == 0 {
                return Ok(UnixStream::from(fd));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINPROGRESS) {
                loop {
                    ensure!(Instant::now() < deadline, "IPC connect timed out");
                    let mut poll = libc::pollfd {
                        fd: fd.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    let millis = deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(i32::MAX as u128) as i32;
                    // SAFETY: one initialized pollfd lives throughout the bounded call.
                    let result = unsafe { libc::poll(&mut poll, 1, millis) };
                    if result < 0 {
                        let error = io::Error::last_os_error();
                        if error.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(error.into());
                    }
                    ensure!(result > 0, "IPC connect timed out");
                    let mut error: libc::c_int = 0;
                    let mut length = std::mem::size_of_val(&error) as libc::socklen_t;
                    // SAFETY: correctly sized writable error/length pointers and live fd.
                    ensure!(
                        unsafe {
                            libc::getsockopt(
                                fd.as_raw_fd(),
                                libc::SOL_SOCKET,
                                libc::SO_ERROR,
                                (&mut error as *mut libc::c_int).cast(),
                                &mut length,
                            )
                        } == 0,
                        "read socket connection result: {}",
                        io::Error::last_os_error()
                    );
                    ensure!(
                        error == 0,
                        "connect Unix socket: {}",
                        io::Error::from_raw_os_error(error)
                    );
                    return Ok(UnixStream::from(fd));
                }
            }
            if error.kind() != io::ErrorKind::WouldBlock
                && error.kind() != io::ErrorKind::Interrupted
            {
                return Err(error.into());
            }
            // A failed connect's state is unspecified: close it and retry on a
            // fresh socket, always against the original bounded deadline.
            drop(fd);
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use std::{
        cell::Cell,
        hash::{Hash, Hasher},
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle},
        },
    };
    use windows_sys::Win32::{
        Foundation::{
            ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
            ERROR_PIPE_LISTENING, INVALID_HANDLE_VALUE,
        },
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, ReadFile},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT,
            PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            SetNamedPipeHandleState,
        },
    };
    fn name(local: &Path) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        local
            .as_os_str()
            .encode_wide()
            .collect::<Vec<_>>()
            .hash(&mut hash);
        format!(r"\\.\pipe\mai-{:016x}", hash.finish())
    }
    fn create(local: &Path) -> Result<File> {
        let name: Vec<u16> = name(local).encode_utf16().chain(Some(0)).collect();
        // SAFETY: name is NUL terminated and lives for the call; no security
        // descriptor or overlapped pointer is supplied. File owns the handle.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_FRAME as u32 + 1,
                MAX_FRAME as u32 + 1,
                0,
                std::ptr::null(),
            )
        };
        ensure!(
            handle != INVALID_HANDLE_VALUE,
            "create local named pipe: {}",
            io::Error::last_os_error()
        );
        Ok(unsafe { File::from_raw_handle(handle) })
    }
    pub struct Listener {
        file: File,
        connected: Cell<bool>,
    }
    impl Listener {
        pub fn bind(local: &Path) -> Result<Self> {
            Ok(Self {
                file: create(local)?,
                connected: Cell::new(false),
            })
        }
        pub fn accept(&self) -> Result<Option<Stream>> {
            if self.connected.replace(false) {
                // SAFETY: listener retains the original valid server handle.
                // The previous response has been acknowledged or timed out.
                // Reuse this instance even while a previous client closes its
                // handle; creating another FIRST_PIPE_INSTANCE would race it.
                unsafe {
                    DisconnectNamedPipe(self.file.as_raw_handle());
                }
            }
            // SAFETY: File owns a valid named pipe; nonblocking mode needs no OVERLAPPED.
            let connected =
                unsafe { ConnectNamedPipe(self.file.as_raw_handle(), std::ptr::null_mut()) };
            if connected != 0 {
                return Ok(None);
            } // PIPE_NOWAIT transitioned to listening.
            let error = io::Error::last_os_error();
            match error.raw_os_error().map(|code| code as u32) {
                Some(ERROR_PIPE_CONNECTED) => {
                    self.connected.set(true);
                    Ok(Some(Stream(self.file.try_clone()?)))
                }
                Some(ERROR_PIPE_LISTENING) => Ok(None),
                Some(ERROR_NO_DATA) => {
                    // SAFETY: a client closed before acceptance; reset the
                    // owned instance so the next client can connect.
                    unsafe {
                        DisconnectNamedPipe(self.file.as_raw_handle());
                    }
                    Ok(None)
                }
                _ => Err(error.into()),
            }
        }
    }
    pub struct Stream(File);
    fn map_nonblocking<T>(result: io::Result<T>) -> io::Result<T> {
        result.map_err(|error| {
            if error.raw_os_error() == Some(ERROR_NO_DATA as i32) {
                io::Error::from(io::ErrorKind::WouldBlock)
            } else {
                error
            }
        })
    }
    impl Read for Stream {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            let mut count = 0;
            // SAFETY: valid synchronous pipe handle and writable buffer/count;
            // PIPE_NOWAIT bounds this call and no OVERLAPPED is required.
            // Do not use File::read: Rust maps ERROR_NO_DATA to BrokenPipe and
            // then EOF, losing the distinction between idle and disconnected.
            let success = unsafe {
                ReadFile(
                    self.0.as_raw_handle(),
                    bytes.as_mut_ptr(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    &mut count,
                    std::ptr::null_mut(),
                )
            };
            if success != 0 {
                return Ok(count as usize);
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error().map(|code| code as u32) {
                Some(ERROR_NO_DATA) => Err(io::ErrorKind::WouldBlock.into()),
                Some(ERROR_BROKEN_PIPE) => Ok(0),
                _ => Err(error),
            }
        }
    }
    impl Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match map_nonblocking(self.0.write(bytes)) {
                Ok(0) if !bytes.is_empty() => Err(io::ErrorKind::WouldBlock.into()),
                result => result,
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    pub fn connect(local: &Path, deadline: Instant) -> Result<Stream> {
        let file = loop {
            ensure!(Instant::now() < deadline, "IPC connect timed out");
            match OpenOptions::new().read(true).write(true).open(name(local)) {
                Ok(file) => break file,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    // The previous acknowledged client may still be closing.
                    // Respect the caller's original deadline while the single
                    // server instance disconnects and becomes available again.
                    thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        // SAFETY: valid pipe handle and mode pointer; optional output pointers absent.
        ensure!(
            unsafe {
                SetNamedPipeHandleState(
                    file.as_raw_handle(),
                    &mode,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } != 0,
            "set nonblocking pipe mode: {}",
            io::Error::last_os_error()
        );
        Ok(Stream(file))
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        fn accept(listener: &Listener) -> Stream {
            let deadline = Instant::now() + IO_TIMEOUT;
            loop {
                if let Some(stream) = listener.accept().unwrap() {
                    return stream;
                }
                assert!(Instant::now() < deadline, "named pipe was not accepted");
                thread::sleep(POLL);
            }
        }
        #[test]
        fn idle_pipe_read_is_would_block_and_instance_survives_old_client_handle() {
            let temp = tempfile::tempdir().unwrap();
            let listener = Listener::bind(temp.path()).unwrap();
            let mut client = connect(temp.path(), Instant::now() + IO_TIMEOUT).unwrap();
            let mut server = accept(&listener);
            let start = Instant::now();
            assert!(connect(temp.path(), start + Duration::from_millis(40)).is_err());
            assert!(start.elapsed() < Duration::from_secs(1));
            let mut byte = [0];
            assert_eq!(
                client.read(&mut byte).unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            assert_eq!(
                server.read(&mut byte).unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            client.write_all(b"x").unwrap();
            assert_eq!(server.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [b'x']);
            server.write_all(b"y").unwrap();
            assert_eq!(client.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [b'y']);
            drop(server);
            // Keep the old client alive deliberately. The listener resets and
            // reuses its instance instead of racing FIRST_PIPE_INSTANCE.
            assert!(listener.accept().unwrap().is_none());
            let mut next = connect(temp.path(), Instant::now() + IO_TIMEOUT).unwrap();
            let mut server = accept(&listener);
            next.write_all(b"z").unwrap();
            assert_eq!(server.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [b'z']);
            drop(next);
            assert_eq!(server.read(&mut byte).unwrap(), 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn integration_reply_fixture(
        error: Option<&str>,
        delay: Duration,
        close_after_reply: bool,
    ) -> Result<serde_json::Value> {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join(".mai");
        fs::create_dir(&local).unwrap();
        let local = local.canonicalize().unwrap();
        let listener = transport::Listener::bind(&local).unwrap();
        let authority = NativePath::from_path(local.parent().unwrap()).unwrap();
        let error = error.map(str::to_owned);
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut stream = loop {
                if let Some(stream) = listener.accept().unwrap() {
                    break stream;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(POLL);
            };
            let request: Request = read_frame(&mut stream, deadline).unwrap();
            assert!(matches!(request.action, Action::Integrate { .. }));
            if !delay.is_zero() {
                // A received request with no timely reply remains uncertain.
                thread::sleep(delay);
                return;
            }
            write_frame(
                &mut stream,
                &Reply {
                    protocol_version: PROTOCOL_VERSION,
                    authority,
                    result: None,
                    error,
                },
                deadline,
            )
            .unwrap();
            if !close_after_reply {
                let _ = read_frame::<Request>(&mut stream, deadline);
            }
        });
        let result = integrate(
            temp.path(),
            GenerationId {
                change_id: "change".into(),
                generation: 1,
            },
            "textual".into(),
            vec![],
            1,
        );
        server.join().unwrap();
        result
    }

    #[test]
    fn integration_confirmed_remote_rejections_do_not_claim_transport_uncertainty() {
        for message in [
            "unknown submission generation",
            "dependency blocked; integrate the submitted parent before retrying this change",
        ] {
            for close_after_reply in [false, true] {
                let error =
                    integration_reply_fixture(Some(message), Duration::ZERO, close_after_reply)
                        .unwrap_err();
                assert_eq!(format!("{error:#}"), message);
            }
        }
    }

    #[test]
    fn integration_lost_response_preserves_query_before_retry_guidance() {
        let error =
            integration_reply_fixture(None, Duration::from_millis(1200), false).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("response unavailable"), "{message}");
        assert!(message.contains("outcome may be uncertain"), "{message}");
        assert!(
            message.contains("query the same generation before retrying"),
            "{message}"
        );
    }

    #[test]
    fn approval_routes_only_native_ownership_contention() {
        assert!(ownership_contended(&fs2::lock_contended_error()));
        assert!(ownership_contended(&io::Error::from(
            io::ErrorKind::WouldBlock
        )));
        assert!(!ownership_contended(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
    }

    #[cfg(unix)]
    #[test]
    fn connect_deadline_survives_a_full_listener_backlog() {
        use std::os::{fd::AsRawFd, unix::net::UnixListener};
        let temp = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(temp.path().join("engine.sock")).unwrap();
        // SAFETY: valid listening socket; reduce backlog to saturate deterministically.
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0);
        let mut clients = Vec::new();
        for _ in 0..32 {
            match transport::connect(temp.path(), Instant::now() + Duration::from_millis(50)) {
                Ok(client) => clients.push(client),
                Err(_) => break,
            }
        }
        assert!(
            !clients.is_empty() && clients.len() < 32,
            "failed to saturate listener backlog"
        );
        let start = Instant::now();
        assert!(transport::connect(temp.path(), start + Duration::from_millis(150)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn frames_reject_bad_versions_fields_truncation_and_oversize() {
        let deadline = Instant::now() + Duration::from_secs(1);
        assert!(
            read_frame::<Request>(
                &mut b"{\"protocol_version\":1,\"action\":\"status\"}\n".as_slice(),
                deadline
            )
            .is_ok()
        );
        for payload in [
            b"{}\n".as_slice(),
            b"{\"protocol_version\":1,\"action\":\"receive\"}\n",
            b"{\"protocol_version\":1,\"action\":\"status\",\"extra\":1}\n",
            b"{",
        ] {
            assert!(read_frame::<Request>(&mut &*payload, deadline).is_err());
        }
        assert!(
            read_frame::<Request>(&mut vec![b'x'; MAX_FRAME + 1].as_slice(), deadline).is_err()
        );
    }
}
