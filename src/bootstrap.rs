//! Trusted control-plane bootstrap with input and host-tool invalidation identities.
use crate::{
    policy::{CommandPolicy, TrustedPolicy, checked_file},
    process::{self, CancellationToken, ProcessOutcome, RunResult, RunSpec},
};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap, ffi::OsString, io::Write, path::Path, process::Stdio, time::Duration,
};

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
    let mut identities = vec![serde_json::to_string(command)?];
    for name in std::iter::once(&command.script).chain(command.executable_dependencies.iter()) {
        identities.push(hash_file(&checked_file(policy.root(), name)?)?);
    }
    for name in &command.inputs {
        identities.push(hash_file(&checked_file(cwd, name)?)?);
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
        let result = process::run(&spec, &CancellationToken::default())?;
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
        identities.push(hash_file(&executable)?);
    }
    hash_bytes(serde_json::to_vec(&identities)?.as_slice())
}
fn hash_file(path: &Path) -> Result<String> {
    let result = crate::policy::safe_git()
        .args(["hash-object", "--no-filters", "--"])
        .arg(path)
        .output()?;
    ensure!(
        result.status.success(),
        "cannot fingerprint bootstrap input"
    );
    Ok(String::from_utf8(result.stdout)?.trim().into())
}
fn hash_bytes(bytes: &[u8]) -> Result<String> {
    let mut child = crate::policy::safe_git()
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(bytes)?;
    let result = child.wait_with_output()?;
    ensure!(
        result.status.success(),
        "cannot fingerprint bootstrap configuration"
    );
    Ok(String::from_utf8(result.stdout)?.trim().into())
}
