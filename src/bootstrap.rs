//! Trusted control-plane bootstrap with input and host-tool invalidation identities.
use crate::{
    policy::{CommandPolicy, TrustedPolicy, checked_file},
    process::{self, CancellationToken, ProcessOutcome, RunResult, RunSpec},
};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, ffi::OsString, path::Path, time::Duration};

pub fn run(
    policy: &TrustedPolicy,
    command: &CommandPolicy,
    cwd: &Path,
    environment: BTreeMap<OsString, OsString>,
) -> Result<RunResult> {
    let temporary = tempfile::Builder::new()
        .prefix("mai-bootstrap-")
        .tempdir()?;
    let mut spec = policy.command_spec(command, cwd)?;
    spec.env.extend(environment);
    for name in ["TMPDIR", "TMP", "TEMP"] {
        spec.env
            .insert(name.into(), temporary.path().as_os_str().to_owned());
    }
    process::run(&spec, &CancellationToken::default())
}

/// Include actual candidate inputs and trusted executable contents. A matching
/// fingerprint is only reusable after bootstrap succeeded; it is never gate evidence.
pub fn fingerprint(policy: &TrustedPolicy, command: &CommandPolicy, cwd: &Path) -> Result<String> {
    fingerprint_with_cancel(policy, command, cwd, &CancellationToken::default())
}

pub fn fingerprint_with_cancel(
    policy: &TrustedPolicy,
    command: &CommandPolicy,
    cwd: &Path,
    cancellation: &CancellationToken,
) -> Result<String> {
    fingerprint_with_runner(policy, command, cwd, cancellation, |spec, cancellation| {
        process::run(spec, cancellation)
    })
}

pub fn fingerprint_with_runner(
    policy: &TrustedPolicy,
    command: &CommandPolicy,
    cwd: &Path,
    cancellation: &CancellationToken,
    mut run: impl FnMut(&RunSpec, &CancellationToken) -> Result<RunResult>,
) -> Result<String> {
    let mut identities = vec![serde_json::to_string(command)?];
    for name in std::iter::once(&command.script).chain(command.executable_dependencies.iter()) {
        identities.push(hash_file(
            &checked_file(policy.root(), name)?,
            cancellation,
            &mut run,
        )?);
    }
    for name in &command.inputs {
        policy.check_candidate_input(name)?;
        identities.push(hash_file(
            &checked_file(cwd, name)?,
            cancellation,
            &mut run,
        )?);
    }
    for tool in std::iter::once(&command.program).chain(command.tools.iter()) {
        let executable = crate::policy::host_tool(tool)?;
        let spec = RunSpec {
            program: executable.as_os_str().to_owned(),
            args: vec!["--version".into()],
            cwd: policy.root().into(),
            env: BTreeMap::new(),
            timeout: Duration::from_secs(5),
            output_tail_bytes: 4096,
        };
        let result = run(&spec, cancellation)?;
        ensure!(
            matches!(
                result.outcome,
                ProcessOutcome::Success | ProcessOutcome::Failure
            ),
            "cannot identify bootstrap tool {tool}"
        );
        identities.push(serde_json::to_string(&(
            tool,
            result.outcome,
            result.exit_code,
            result.stdout_tail,
            result.stderr_tail,
        ))?);
        identities.push(serde_json::to_string(
            &crate::native_path::NativePath::from_path(&executable.canonicalize()?)?,
        )?);
        identities.push(hash_file(&executable, cancellation, &mut run)?);
    }
    hash_bytes(
        serde_json::to_vec(&identities)?.as_slice(),
        cancellation,
        &mut run,
    )
}
fn hash_file(
    path: &Path,
    cancellation: &CancellationToken,
    run: &mut impl FnMut(&RunSpec, &CancellationToken) -> Result<RunResult>,
) -> Result<String> {
    let result = run(
        &RunSpec {
            program: "git".into(),
            args: vec![
                "--literal-pathspecs".into(),
                "-c".into(),
                "core.hooksPath=".into(),
                "-c".into(),
                "core.fsmonitor=false".into(),
                "-c".into(),
                "protocol.allow=never".into(),
                "hash-object".into(),
                "--no-filters".into(),
                "--".into(),
                crate::repository_scope::external_git_path(path),
            ],
            cwd: std::env::current_dir()?,
            env: [
                ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
                (
                    "GIT_CONFIG_GLOBAL".into(),
                    if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
                ),
                ("GIT_NO_REPLACE_OBJECTS".into(), "1".into()),
                ("GIT_NO_LAZY_FETCH".into(), "1".into()),
                ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ]
            .into(),
            timeout: Duration::from_secs(30),
            output_tail_bytes: 4096,
        },
        cancellation,
    )?;
    ensure!(
        result.outcome == ProcessOutcome::Success,
        "cannot fingerprint bootstrap input"
    );
    Ok(result.stdout_tail.trim().into())
}
fn hash_bytes(
    bytes: &[u8],
    cancellation: &CancellationToken,
    run: &mut impl FnMut(&RunSpec, &CancellationToken) -> Result<RunResult>,
) -> Result<String> {
    let file = tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), bytes)?;
    let result = run(
        &RunSpec {
            program: "git".into(),
            args: vec![
                "--literal-pathspecs".into(),
                "-c".into(),
                "core.hooksPath=".into(),
                "-c".into(),
                "core.fsmonitor=false".into(),
                "-c".into(),
                "protocol.allow=never".into(),
                "hash-object".into(),
                "--no-filters".into(),
                "--".into(),
                crate::repository_scope::external_git_path(file.path()),
            ],
            cwd: std::env::current_dir()?,
            env: [
                ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
                (
                    "GIT_CONFIG_GLOBAL".into(),
                    if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
                ),
                ("GIT_NO_REPLACE_OBJECTS".into(), "1".into()),
                ("GIT_NO_LAZY_FETCH".into(), "1".into()),
                ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ]
            .into(),
            timeout: Duration::from_secs(30),
            output_tail_bytes: 4096,
        },
        cancellation,
    )?;
    ensure!(
        result.outcome == ProcessOutcome::Success,
        "cannot fingerprint bootstrap configuration"
    );
    Ok(result.stdout_tail.trim().into())
}
