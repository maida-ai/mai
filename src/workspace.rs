//! Independent agent clones. Failed creation/bootstrap is preserved for inspection;
//! retrying never claims or overwrites an existing destination.
use crate::{
    authority::Authority, native_path::NativePath, policy::TrustedPolicy, process::RunResult,
    repository_scope::external_git_path,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    path::Path,
    process::Command,
};

#[derive(Debug, Serialize)]
pub struct WorkspaceResult {
    pub schema_version: u32,
    pub destination: NativePath,
    pub agent: String,
    pub base: String,
    pub bootstrap: Option<RunResult>,
    pub bootstrap_skipped: bool,
}

pub fn create(
    project: &Path,
    agent: &str,
    destination: &Path,
    skip_bootstrap: bool,
) -> Result<WorkspaceResult> {
    ensure!(
        !agent.is_empty()
            && agent.len() <= 128
            && agent
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c)),
        "invalid agent identifier"
    );
    ensure!(
        fs::symlink_metadata(destination).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "destination already exists; preserve it and choose a new destination"
    );
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .context("workspace destination parent must exist")?;
    let destination = parent.join(
        destination
            .file_name()
            .context("workspace destination must name a directory")?,
    );
    let authority = Authority::open(project)?;
    let base = git(
        &authority.git_dir(),
        &[
            OsStr::new("rev-parse"),
            OsStr::new("refs/heads/trunk^{commit}"),
        ],
    )?;
    // Reserve exclusively before spawning Git. Every subsequent failure keeps this
    // recognizable directory; even an empty existing destination is never reused.
    fs::create_dir(&destination)?;
    let marker = destination.join(".mai-workspace-incomplete");
    fs::write(
        &marker,
        "Workspace creation incomplete; preserve for inspection and retry in a new destination.\n",
    )?;
    // Git requires an empty destination. Clone into a child and move it only after
    // success; the exclusive parent reservation survives partial-clone failures.
    let staging = tempfile::Builder::new()
        .prefix(".mai-workspace-stage-")
        .tempdir_in(&parent)?;
    let stage = staging.path().join("clone");
    let output = git_command()
        .args([
            "clone",
            "--no-checkout",
            "--no-local",
            "--no-hardlinks",
            "--origin",
            "mai",
            "--",
        ])
        .arg(external_git_path(&authority.git_dir()))
        .arg(external_git_path(&stage))
        .output()
        .context("start independent workspace clone")?;
    ensure!(
        output.status.success(),
        "workspace clone failed; preserved destination: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for entry in fs::read_dir(&stage)? {
        let entry = entry?;
        ensure!(
            entry.file_name() != OsStr::new(".mai-workspace-incomplete"),
            "source contains reserved workspace marker"
        );
        fs::rename(entry.path(), destination.join(entry.file_name()))?;
    }
    fs::remove_dir(&stage)?;
    git(
        &destination,
        &[
            OsStr::new("config"),
            OsStr::new("mai.agent"),
            OsStr::new(agent),
        ],
    )?;
    git(
        &destination,
        &[
            OsStr::new("checkout"),
            OsStr::new("--detach"),
            OsStr::new(&base),
        ],
    )?;
    let has_policy = git_command()
        .arg("-C")
        .arg(&destination)
        .args(["cat-file", "-e", &format!("{base}:mai.toml")])
        .output()?
        .status
        .success();
    let bootstrap = if !skip_bootstrap && has_policy {
        let trusted = TrustedPolicy::load(&authority, &base)?;
        trusted
            .config
            .workspaces
            .bootstrap
            .as_ref()
            .map(|command| {
                crate::bootstrap::run(
                    &trusted,
                    command,
                    &destination,
                    BTreeMap::<OsString, OsString>::new(),
                )
            })
            .transpose()?
    } else {
        None
    };
    if bootstrap
        .as_ref()
        .is_some_and(|result| result.outcome != crate::process::ProcessOutcome::Success)
    {
        fs::write(
            &marker,
            "Workspace clone complete; bootstrap failed. Inspect structured diagnostics and preserve this destination. Retry creation in a new destination.\n",
        )?;
    } else {
        fs::remove_file(marker)?;
    }
    Ok(WorkspaceResult {
        schema_version: 1,
        destination: NativePath::from_path(&destination)?,
        agent: agent.into(),
        base,
        bootstrap,
        bootstrap_skipped: skip_bootstrap,
    })
}
fn git(path: &Path, args: &[&OsStr]) -> Result<String> {
    let output = git_command()
        .arg("-C")
        .arg(external_git_path(path))
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "workspace Git operation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

pub(crate) fn git_command() -> Command {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .args(["-c", "core.hooksPath=", "-c", "core.fsmonitor=false"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}
