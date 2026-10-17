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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_diagnostic: Option<String>,
}
const CLEANUP_DIAGNOSTIC_BYTES: usize = 2048;
fn bounded_cleanup_message(mut message: String) -> String {
    let mut end = message.len().min(CLEANUP_DIAGNOSTIC_BYTES);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message.truncate(end);
    message
}
fn cleanup_note(diagnostic: &mut Option<String>, message: String) {
    let message = bounded_cleanup_message(message);
    *diagnostic = Some(bounded_cleanup_message(match diagnostic.take() {
        Some(previous) => format!("{previous}; {message}"),
        None => message,
    }));
}
#[derive(Default)]
struct CleanupConfirmation {
    confirmed: bool,
    last_error: Option<String>,
}
impl CleanupConfirmation {
    fn observe(&mut self, probe: impl FnOnce() -> Result<bool>) {
        if self.confirmed {
            return;
        }
        match probe() {
            Ok(confirmed) => {
                self.confirmed = confirmed;
                if confirmed {
                    self.last_error = None;
                }
            }
            Err(error) => {
                self.confirmed = false;
                self.last_error = Some(bounded_cleanup_message(format!("{error:#}")));
            }
        }
    }
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
            cleanup_diagnostic: None,
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
    let mut cleanup_diagnostic = None;
    if let Err(error) = supervision.terminate() {
        outcome = ProcessOutcome::CleanupFailure;
        cleanup_note(
            &mut cleanup_diagnostic,
            format!("terminate descendants: {error:#}"),
        );
    }
    let cleanup_start = Instant::now();
    let mut child_done = false;
    let mut confirmation = CleanupConfirmation::default();
    while cleanup_start.elapsed() < Duration::from_secs(1) {
        match child.try_wait() {
            Ok(status) => child_done = status.is_some(),
            Err(error) => {
                cleanup_note(
                    &mut cleanup_diagnostic,
                    format!("wait for direct child: {error}"),
                );
                break;
            }
        }
        if child_done && out_reader.is_finished() && err_reader.is_finished() {
            confirmation.observe(|| supervision.quiescent());
            if confirmation.confirmed {
                break;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    // No second probe: after confirmed quiescence no group member can fork,
    // and another /proc scan could only add an unrelated disappearing-PID race.
    if !child_done {
        outcome = ProcessOutcome::CleanupFailure;
        cleanup_note(
            &mut cleanup_diagnostic,
            "direct child did not finish within cleanup bound".into(),
        );
    }
    if !confirmation.confirmed {
        outcome = ProcessOutcome::CleanupFailure;
        let detail = confirmation
            .last_error
            .unwrap_or_else(|| "process group/job or output readers still active".into());
        cleanup_note(
            &mut cleanup_diagnostic,
            format!("quiescence not confirmed within cleanup bound: {detail}"),
        );
    }
    for (stream, reader) in [("stdout", out_reader), ("stderr", err_reader)] {
        // A deliberately escaped process can retain output handles. Never let
        // that convert a bounded command into an unbounded reader join.
        let failure = if !reader.is_finished() {
            Some(format!(
                "{stream} reader did not finish within cleanup bound"
            ))
        } else {
            match reader.join() {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(format!("{stream} reader failed: {error}")),
                Err(_) => Some(format!("{stream} reader panicked")),
            }
        };
        if let Some(failure) = failure {
            outcome = ProcessOutcome::CleanupFailure;
            cleanup_note(&mut cleanup_diagnostic, failure);
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
        cleanup_diagnostic,
    })
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn confirmed_quiescence_is_never_invalidated_by_an_unrelated_later_probe() {
        let mut confirmation = CleanupConfirmation::default();
        confirmation.observe(|| Ok(true));
        let mut called = false;
        confirmation.observe(|| {
            called = true;
            anyhow::bail!("unrelated /proc entry vanished")
        });
        assert!(
            !called,
            "confirmed cleanup must not inspect unrelated processes again"
        );
        assert!(confirmation.confirmed);
        assert!(confirmation.last_error.is_none());
    }
    #[test]
    fn persistent_probe_errors_stay_fail_closed_and_diagnostics_are_bounded() {
        let mut confirmation = CleanupConfirmation::default();
        confirmation.observe(|| anyhow::bail!("permission denied inspecting process group"));
        assert!(!confirmation.confirmed);
        assert!(
            confirmation
                .last_error
                .as_deref()
                .unwrap()
                .contains("permission denied")
        );
        let mut diagnostic = None;
        cleanup_note(
            &mut diagnostic,
            "quiescence probe failed: ".to_owned() + &"界".repeat(4096),
        );
        cleanup_note(&mut diagnostic, "second cleanup failure".into());
        let diagnostic = diagnostic.unwrap();
        assert!(diagnostic.len() <= CLEANUP_DIAGNOSTIC_BYTES);
        assert!(diagnostic.starts_with("quiescence probe failed:"));
    }
}
