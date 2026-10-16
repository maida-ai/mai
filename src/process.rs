//! Bounded native commands for a trusted local service, not an OS sandbox.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsString,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod windows_arguments;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

#[derive(Clone, Debug)]
pub struct RunSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub timeout: Duration,
    pub output_tail_bytes: usize,
}
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutcome {
    Success,
    Failure,
    Timeout,
    Cancelled,
    CleanupFailure,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunResult {
    pub outcome: ProcessOutcome,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub stdout_tail: String,
    pub stderr_tail: String,
}
fn tail(
    mut input: impl Read,
    capacity: usize,
    output: Arc<Mutex<VecDeque<u8>>>,
) -> std::io::Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let mut output = output
            .lock()
            .map_err(|_| std::io::Error::other("output mutex poisoned"))?;
        for byte in &buffer[..count] {
            if output.len() == capacity {
                output.pop_front();
            }
            if capacity > 0 {
                output.push_back(*byte);
            }
        }
    }
    Ok(())
}
fn captured(output: &Mutex<VecDeque<u8>>) -> String {
    match output.lock() {
        Ok(mut output) => String::from_utf8_lossy(output.make_contiguous()).into_owned(),
        Err(_) => String::new(),
    }
}
/// Always terminates remaining descendants before returning, including when the
/// direct command succeeded. Escaping process groups deliberately is unsupported.
pub fn run(spec: &RunSpec, cancellation: &CancellationToken) -> Result<RunResult> {
    ensure!(!spec.timeout.is_zero(), "process timeout must be positive");
    ensure!(
        spec.output_tail_bytes <= 16 * 1024 * 1024,
        "output tail exceeds 16 MiB per stream"
    );
    let start = Instant::now();
    if cancellation.is_cancelled() {
        return Ok(RunResult {
            outcome: ProcessOutcome::Cancelled,
            exit_code: None,
            elapsed_ms: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        });
    }
    let mut command = Command::new(&spec.program);
    // Caller environment must not redirect Git commands to another repository
    // or inject command-scoped config. Explicit per-run values remain possible.
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(name);
        }
    }
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    let (mut supervision, mut child) = {
        let mut supervision = platform::Supervision::prepare(&mut command)?;
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn bounded command {:?}", spec.program))?;
        supervision.attach(&mut child)?;
        (supervision, child)
    };
    #[cfg(windows)]
    let (mut supervision, mut child) = platform::spawn(&command)?;
    let stdout = child.stdout.take().context("missing command stdout")?;
    let stderr = child.stderr.take().context("missing command stderr")?;
    let capacity = spec.output_tail_bytes;
    let out = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));
    let err = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));
    let out_copy = out.clone();
    let err_copy = err.clone();
    let out_reader = thread::spawn(move || tail(stdout, capacity, out_copy));
    let err_reader = thread::spawn(move || tail(stderr, capacity, err_copy));
    let (mut outcome, exit_code) = loop {
        if let Some(status) = child.try_wait()? {
            break (
                if status.success() {
                    ProcessOutcome::Success
                } else {
                    ProcessOutcome::Failure
                },
                status.code(),
            );
        }
        if cancellation.is_cancelled() {
            break (ProcessOutcome::Cancelled, None);
        }
        if start.elapsed() >= spec.timeout {
            break (ProcessOutcome::Timeout, None);
        }
        thread::sleep(Duration::from_millis(5));
    };
    if supervision.terminate().is_err() {
        outcome = ProcessOutcome::CleanupFailure;
    }
    let cleanup_start = Instant::now();
    let mut child_done = false;
    while cleanup_start.elapsed() < Duration::from_secs(1) {
        child_done = child.try_wait()?.is_some();
        if child_done
            && out_reader.is_finished()
            && err_reader.is_finished()
            && supervision.quiescent().unwrap_or(false)
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    if !child_done || !supervision.quiescent().unwrap_or(false) {
        outcome = ProcessOutcome::CleanupFailure;
    }
    for reader in [out_reader, err_reader] {
        // A deliberately escaped process can retain output handles. Never let
        // that convert a bounded command into an unbounded reader join.
        if !reader.is_finished() || !matches!(reader.join(), Ok(Ok(()))) {
            outcome = ProcessOutcome::CleanupFailure;
        }
    }
    let stdout_tail = captured(&out);
    let stderr_tail = captured(&err);
    Ok(RunResult {
        outcome,
        exit_code,
        elapsed_ms: start.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        stdout_tail,
        stderr_tail,
    })
}
