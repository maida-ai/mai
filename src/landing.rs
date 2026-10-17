//! Candidate construction only. A prepared tree is not validation or permission to publish.
//! Callers serialize against the engine and must recheck trunk, policy and leases before CAS.
use crate::{
    authority::Authority,
    lifecycle::{GenerationId, Lifecycle, Outcome, State},
    repository_scope::external_git_path,
};
use anyhow::{Context, Result, ensure};
use std::process::{Command, Output};

/// Explicit opt-in to Git's textual three-way merge. No mode default is selected here.
#[derive(Clone, Copy, Debug)]
pub enum MergeMode {
    Textual,
}

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
        matches!(submission.state(), State::Queued | State::Validating { .. }),
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
            &format!("--merge-base={base}"),
            &trunk,
            &submitted,
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "candidate merge conflicted or Git merge-tree failed"
    );
    let text = String::from_utf8(output.stdout)?;
    let tree = text
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
