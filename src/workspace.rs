//! Independent agent clones. Failed creation/bootstrap is preserved for inspection;
//! retrying never claims or overwrites an existing destination.
use crate::{
    authority::Authority, native_path::NativePath, policy::TrustedPolicy, process::RunResult,
    repository_scope::external_git_path,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContinuationMarker {
    schema_version: u32,
    owner: String,
    authority: NativePath,
    source: NativePath,
    change: String,
    generation: u64,
    submitted_commit: String,
    landed_commit: String,
    base_trunk: String,
    suffix: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ContinuationResult {
    pub schema_version: u32,
    pub destination: NativePath,
    pub agent: String,
    pub change: String,
    pub generation: u64,
    pub submitted_commit: String,
    pub landed_commit: String,
    pub base_trunk: String,
    pub replayed: Vec<ReplayedCommit>,
    pub replay_committer: String,
    pub bootstrap: Option<RunResult>,
    pub bootstrap_skipped: bool,
    pub next_step: String,
}

#[derive(Debug, Serialize)]
pub struct ReplayedCommit {
    pub source_commit: String,
    pub destination_commit: String,
}

struct SourceSnapshot {
    root: PathBuf,
    git_dir: PathBuf,
    head: String,
    index: Vec<u8>,
    config: Vec<u8>,
    refs: String,
    status: String,
}

/// Create a fresh workspace from the accepted trunk and replay only the
/// verified, clean, first-parent suffix after one exact landed submission.
pub fn continue_landed(
    project: &Path,
    source: &Path,
    destination: &Path,
    change: &str,
    generation: u64,
    skip_bootstrap: bool,
) -> Result<ContinuationResult> {
    ensure!(generation > 0, "generation must be positive");
    ensure!(
        !change.is_empty()
            && change.len() <= 128
            && change
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c)),
        "invalid change identifier"
    );
    let authority = Authority::open(project)?;
    let authority_root = authority.root().canonicalize()?;
    let authority_git = authority.git_dir().canonicalize()?;
    let source = inspect_source(source, &authority_git)?;
    let generation_id = crate::lifecycle::GenerationId {
        change_id: change.to_owned(),
        generation,
    };
    let initial =
        crate::engine::continuation_snapshot(&authority_root, generation_id.clone(), Vec::new())?;
    let (suffix, submitted_commit) = source_suffix(&source, &initial.submitted_commit)?;
    let snapshot =
        crate::engine::continuation_snapshot(&authority_root, generation_id, suffix.clone())?;
    ensure!(
        snapshot.agent == initial.agent
            && snapshot.submitted_commit == initial.submitted_commit
            && snapshot.landed_commit == initial.landed_commit
            && snapshot.revision == initial.revision,
        "selected generation identity changed while inspecting the source; retry after verifying authority state"
    );
    ensure!(
        snapshot.agent
            == String::from_utf8(
                git_config_file(&source.git_dir.join("config"), &["--get", "mai.agent"],)?.stdout,
            )?
            .trim(),
        "source workspace belongs to a different registered agent"
    );
    ensure!(
        snapshot.submitted_commit == submitted_commit,
        "source suffix does not begin at the selected submitted commit"
    );
    ensure_no_replay_attributes(&authority_git, &snapshot.base_trunk, &source.root, &suffix)?;
    verify_source_snapshot(&source)?;

    let destination = destination_path(destination)?;
    ensure_disjoint(&destination, &source.root, &authority_root, &authority_git)?;
    let owner = random_owner()?;
    fs::create_dir(&destination).context("reserve new continuation destination exclusively")?;
    let marker_path = destination.join(".mai-continuation-incomplete");
    let marker = ContinuationMarker {
        schema_version: 1,
        owner: owner.clone(),
        authority: NativePath::from_path(&authority_root)?,
        source: NativePath::from_path(&source.root)?,
        change: change.to_owned(),
        generation,
        submitted_commit: snapshot.submitted_commit.clone(),
        landed_commit: snapshot.landed_commit.clone(),
        base_trunk: snapshot.base_trunk.clone(),
        suffix: suffix.clone(),
    };
    write_marker(&marker_path, &marker)?;
    sync_directory(&destination)?;
    sync_directory(
        destination
            .parent()
            .context("destination must have a parent")?,
    )?;

    // Keep partial clone data under the owned destination so process death
    // cannot leave an unmarked sibling staging tree.
    let stage = destination.join(format!(".mai-continuation-stage-{owner}"));
    fs::create_dir(&stage)?;
    let clone = stage.join("clone");
    let cloned = git_command()
        .args([
            "clone",
            "--no-checkout",
            "--no-local",
            "--no-hardlinks",
            "--origin",
            "mai",
            "--",
        ])
        .arg(external_git_path(&authority_git))
        .arg(external_git_path(&clone))
        .output()
        .context("start continuation destination clone")?;
    ensure!(
        cloned.status.success(),
        "destination clone failed; preserve incomplete destination {} and retry at a new path: {}",
        destination.display(),
        bounded_text(&cloned.stderr)
    );
    for entry in fs::read_dir(&clone)? {
        let entry = entry?;
        ensure!(
            entry.file_name() != OsStr::new(".mai-continuation-incomplete"),
            "authority contains reserved continuation marker"
        );
        fs::rename(entry.path(), destination.join(entry.file_name()))?;
    }
    fs::remove_dir(&clone)?;
    fs::remove_dir(&stage)?;
    git(
        &destination,
        &[
            OsStr::new("checkout"),
            OsStr::new("--detach"),
            OsStr::new(&snapshot.base_trunk),
        ],
    )?;

    let mut replayed = Vec::with_capacity(suffix.len());
    if !suffix.is_empty() {
        let fetched = git_command()
            .arg("-C")
            .arg(external_git_path(&destination))
            .args([
                "fetch",
                "--no-write-fetch-head",
                "--no-tags",
                "--no-recurse-submodules",
                "--",
            ])
            .arg(external_git_path(&source.root))
            .arg(&source.head)
            .output()
            .context("import continuation commit objects into destination")?;
        ensure!(
            fetched.status.success(),
            "could not import source history into destination; preserve incomplete destination {} and retry at a new path: {}",
            destination.display(),
            bounded_text(&fetched.stderr)
        );
        for source_commit in &suffix {
            let cherry = git_command()
                .arg("-C")
                .arg(external_git_path(&destination))
                .args([
                    "-c",
                    if cfg!(windows) {
                        "core.attributesFile=NUL"
                    } else {
                        "core.attributesFile=/dev/null"
                    },
                    "-c",
                    "merge.renormalize=false",
                    "-c",
                    "user.name=Mai Continuation",
                    "-c",
                    "user.email=continuation@mai.invalid",
                    "cherry-pick",
                    "--no-gpg-sign",
                    source_commit,
                ])
                .output()
                .context("replay continuation commit")?;
            if !cherry.status.success() {
                let unmerged = git_bytes(&destination, &["ls-files", "-u"])?;
                if !unmerged.is_empty() {
                    anyhow::bail!(
                        "continuation replay conflicted at {source_commit}; inspect `git -C {} status`. The incomplete destination is preserved for diagnostics; after addressing the cause, retry with a new destination. Source remains untouched. Git: {}",
                        destination.display(),
                        bounded_text(&cherry.stderr)
                    );
                }
                anyhow::bail!(
                    "continuation replay failed at {source_commit} without a content conflict; destination is preserved at {}. Inspect its state and stderr before choosing a new destination: {}",
                    destination.display(),
                    bounded_text(&cherry.stderr)
                );
            }
            replayed.push(ReplayedCommit {
                source_commit: source_commit.clone(),
                destination_commit: git_value(&destination, &["rev-parse", "HEAD"])?,
            });
        }
    }
    git(
        &destination,
        &[
            OsStr::new("config"),
            OsStr::new("mai.agent"),
            OsStr::new(&snapshot.agent),
        ],
    )?;
    let has_policy = git_command()
        .arg("-C")
        .arg(external_git_path(&destination))
        .args([
            "cat-file",
            "-e",
            &format!("{}:mai.toml", snapshot.base_trunk),
        ])
        .output()?
        .status
        .success();
    let bootstrap = if !skip_bootstrap && has_policy {
        let trusted = TrustedPolicy::load(&authority, &snapshot.base_trunk)?;
        ensure!(
            trusted.binding.base == snapshot.base_trunk,
            "trusted trunk advanced during continuation; preserve the incomplete destination and retry from a new path, or explicitly use --skip-bootstrap"
        );
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
    if let Some(result) = bootstrap
        .as_ref()
        .filter(|result| result.outcome != crate::process::ProcessOutcome::Success)
    {
        let failure_path = destination.join(".mai-continuation-failure.json");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&failure_path)?;
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({"schema_version":1,"kind":"bootstrap_failed","result":result}),
        )?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        sync_directory(&destination)?;
        anyhow::bail!(
            "trusted bootstrap failed after replay; destination and incomplete marker are preserved. Inspect {} before retrying at a new destination",
            failure_path.display()
        );
    }
    verify_source_snapshot(&source)?;
    fs::remove_file(&marker_path)
        .context("remove continuation marker only after successful replay and bootstrap")?;
    sync_directory(&destination)?;
    Ok(ContinuationResult {
        schema_version: 1,
        destination: NativePath::from_path(&destination)?,
        agent: snapshot.agent,
        change: change.to_owned(),
        generation,
        submitted_commit: snapshot.submitted_commit,
        landed_commit: snapshot.landed_commit,
        base_trunk: snapshot.base_trunk,
        replayed,
        replay_committer: "Mai Continuation <continuation@mai.invalid>".into(),
        bootstrap,
        bootstrap_skipped: skip_bootstrap,
        next_step: "review the replayed commits, then submit each one as a new submission; the original generation remains immutable".into(),
    })
}

fn inspect_source(source: &Path, authority_git: &Path) -> Result<SourceSnapshot> {
    let root = source
        .canonicalize()
        .context("source workspace must exist")?;
    ensure!(
        fs::symlink_metadata(&root)?.is_dir(),
        "source workspace must be a real directory"
    );
    let dot_git = root.join(".git");
    ensure!(
        fs::symlink_metadata(&dot_git)?.is_dir(),
        "source must be a normal repository, not a linked worktree, gitfile, or symlink"
    );
    let git_dir = dot_git.canonicalize()?;
    ensure!(
        git_dir == root.join(".git"),
        "source Git directory resolves outside the workspace"
    );
    for unsupported in ["commondir", "objects/info/alternates"] {
        ensure!(
            fs::symlink_metadata(git_dir.join(unsupported)).is_err(),
            "source uses unsupported shared Git storage ({unsupported})"
        );
    }
    ensure!(
        fs::symlink_metadata(git_dir.join("objects"))?.is_dir(),
        "source Git objects directory must be local and non-symlink"
    );
    for required in ["config", "index"] {
        ensure!(
            fs::symlink_metadata(git_dir.join(required))?.is_file(),
            "source Git {required} must be a regular file"
        );
    }
    let config_path = git_dir.join("config");
    ensure!(
        fs::symlink_metadata(git_dir.join("HEAD"))?.is_file(),
        "source Git HEAD must be a regular file"
    );
    for optional_metadata in ["packed-refs", "info/exclude"] {
        if let Ok(metadata) = fs::symlink_metadata(git_dir.join(optional_metadata)) {
            ensure!(
                !metadata.file_type().is_symlink() && metadata.is_file(),
                "source Git metadata {optional_metadata} must be a regular file"
            );
        }
    }
    ensure!(
        fs::symlink_metadata(git_dir.join("config.worktree")).is_err(),
        "source contains unsupported linked-worktree config"
    );
    let included = git_config_file(&config_path, &["--get-regexp", r"^(include|includeIf)\."])?;
    ensure!(
        included.status.code() == Some(1),
        "source config includes are unsupported; remove them explicitly before continuing"
    );
    let filters = git_config_file(&config_path, &["--get-regexp", r"^filter\."])?;
    ensure!(
        filters.status.code() == Some(1),
        "source config defines external clean/smudge filters; remove them explicitly before continuing"
    );
    let worktree_config = git_config_file(&config_path, &["--get", "core.worktree"])?;
    ensure!(
        worktree_config.status.code() == Some(1),
        "source config redirects its worktree; linked or redirected layouts are unsupported"
    );
    let extensions_worktree =
        git_config_file(&config_path, &["--get", "extensions.worktreeConfig"])?;
    ensure!(
        extensions_worktree.status.code() == Some(1),
        "source uses unsupported linked-worktree configuration"
    );
    let bare_config = git_config_file(&config_path, &["--get", "core.bare"])?;
    ensure!(
        bare_config.status.code() != Some(0)
            || String::from_utf8_lossy(&bare_config.stdout).trim() != "true",
        "source config marks this repository bare"
    );
    ensure!(
        fs::symlink_metadata(git_dir.join("info/attributes")).is_err(),
        "source repository-local attributes are unsupported"
    );
    for unsupported in ["info/grafts", "shallow"] {
        ensure!(
            fs::symlink_metadata(git_dir.join(unsupported)).is_err(),
            "source uses unsupported Git history rewriting metadata ({unsupported})"
        );
    }
    reject_symlinks_under(&git_dir.join("refs"))?;
    for operation_state in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_LOG",
        "BISECT_START",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
    ] {
        ensure!(
            fs::symlink_metadata(git_dir.join(operation_state)).is_err(),
            "source has an ongoing Git operation ({operation_state}); finish or abort it explicitly before continuing"
        );
    }
    let current_tree = git_bytes(&root, &["ls-tree", "-r", "--name-only", "-z", "HEAD"])?;
    ensure!(
        !has_attributes(&current_tree),
        "source HEAD contains .gitattributes; source status with clean filters is unsupported"
    );
    let remote = git_config_file(&config_path, &["--get-all", "remote.mai.url"])?;
    ensure!(
        remote.status.success(),
        "source has no direct mai authority remote in its local config"
    );
    let remote = String::from_utf8(remote.stdout)?.trim().to_owned();
    ensure!(
        !remote.contains('\n'),
        "source has ambiguous multiple mai remotes"
    );
    let remote_path = Path::new(&remote);
    ensure!(
        remote_path.is_absolute(),
        "mai remote must be a local absolute authority Git path"
    );
    ensure!(
        remote_path.canonicalize()? == authority_git,
        "source mai remote does not identify this authority"
    );
    ensure!(
        PathBuf::from(git_value(&root, &["rev-parse", "--show-toplevel"])?).canonicalize()? == root
            && PathBuf::from(git_value(&root, &["rev-parse", "--absolute-git-dir"])?)
                .canonicalize()?
                == git_dir,
        "source repository config redirects Git outside its normal worktree layout"
    );
    let head = git_value(&root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let status = git_bytes(
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    ensure!(
        status.is_empty(),
        "source workspace has tracked changes or non-ignored untracked files; preserve it and clean explicitly before continuing"
    );
    let flags = git_bytes(&root, &["ls-files", "-v", "-z"])?;
    ensure!(
        flags
            .split(|byte| *byte == 0)
            .all(|entry| entry.is_empty() || entry[0] == b'H'),
        "source index uses assume-unchanged or skip-worktree flags; clear them explicitly before continuing"
    );
    Ok(SourceSnapshot {
        root,
        git_dir: git_dir.clone(),
        head,
        index: fs::read(git_dir.join("index"))?,
        config: fs::read(git_dir.join("config"))?,
        refs: git_dir_value(
            &git_dir,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        )?,
        status: String::from_utf8(status)?,
    })
}

fn source_suffix(source: &SourceSnapshot, submitted: &str) -> Result<(Vec<String>, String)> {
    git_value(
        &source.root,
        &["cat-file", "-e", &format!("{submitted}^{{commit}}")],
    )
    .context("source does not contain the selected submitted commit")?;
    if source.head == submitted {
        return Ok((Vec::new(), submitted.to_owned()));
    }
    let history = git_value(
        &source.root,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            "--max-count=258",
            &source.head,
        ],
    )?;
    let commits: Vec<_> = history.lines().map(str::to_owned).collect();
    let position = commits
        .iter()
        .position(|commit| commit == submitted)
        .context("selected submitted commit is not on source HEAD's first-parent chain")?;
    let suffix = commits[position + 1..].to_vec();
    ensure!(
        suffix.len() <= 256,
        "continuation suffix exceeds 256 commits; split it into a smaller reviewed continuation"
    );
    let mut parent = submitted;
    for commit in &suffix {
        let parents = git_value(&source.root, &["rev-list", "--parents", "-n", "1", commit])?;
        let parts: Vec<_> = parents.split_whitespace().collect();
        ensure!(
            parts.len() == 2 && parts[0] == commit && parts[1] == parent,
            "continuation suffix is not an unambiguous linear chain (merge commits are unsupported)"
        );
        parent = commit;
    }
    Ok((suffix, submitted.to_owned()))
}

fn ensure_no_replay_attributes(
    authority_git: &Path,
    base: &str,
    source: &Path,
    suffix: &[String],
) -> Result<()> {
    ensure!(
        !authority_git.join("info/attributes").exists(),
        "authority repository-local attributes are unsupported for continuation replay"
    );
    for (path, revision) in [(authority_git, base), (source, "HEAD")] {
        let output = if path == authority_git {
            git_dir_bytes(path, &["ls-tree", "-r", "--name-only", "-z", revision])?
        } else {
            git_bytes(path, &["ls-tree", "-r", "--name-only", "-z", revision])?
        };
        ensure!(
            !has_attributes(&output),
            "trusted base or source history contains .gitattributes; continuation replay is unsupported"
        );
    }
    for commit in suffix {
        let paths = git_bytes(
            source,
            &[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-only",
                "-r",
                "-z",
                commit,
            ],
        )?;
        ensure!(
            !has_attributes(&paths),
            "suffix commit {commit} changes .gitattributes; continuation replay is unsupported"
        );
    }
    let drivers = git_dir_command(authority_git)
        .args(["config", "--get-regexp", r"^merge\..*\.driver$"])
        .output()?;
    ensure!(
        drivers.status.code() == Some(1),
        "custom merge drivers are unsupported or authority configuration could not be inspected"
    );
    Ok(())
}

fn has_attributes(paths: &[u8]) -> bool {
    paths
        .split(|byte| *byte == 0)
        .any(|path| path.rsplit(|byte| *byte == b'/').next() == Some(b".gitattributes"))
}

fn verify_source_snapshot(snapshot: &SourceSnapshot) -> Result<()> {
    ensure!(
        git_value(&snapshot.root, &["rev-parse", "--verify", "HEAD^{commit}"])? == snapshot.head,
        "source HEAD changed during continuation; preserve all work and retry"
    );
    ensure!(
        fs::read(snapshot.git_dir.join("index"))? == snapshot.index,
        "source index changed during continuation; preserve all work and retry"
    );
    ensure!(
        fs::read(snapshot.git_dir.join("config"))? == snapshot.config,
        "source Git config changed during continuation; preserve all work and retry"
    );
    ensure!(
        git_dir_value(
            &snapshot.git_dir,
            &["for-each-ref", "--format=%(refname) %(objectname)"]
        )? == snapshot.refs,
        "source refs changed during continuation; preserve all work and retry"
    );
    let current = git_bytes(
        &snapshot.root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    ensure!(
        String::from_utf8(current)? == snapshot.status,
        "source worktree changed during continuation; preserve all work and retry"
    );
    Ok(())
}

fn destination_path(path: &Path) -> Result<PathBuf> {
    ensure!(
        fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "destination already exists; preserve it and choose a new destination"
    );
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .context("destination parent must already exist")?;
    let destination = parent.join(
        path.file_name()
            .context("destination must name a directory")?,
    );
    Ok(destination)
}

fn ensure_disjoint(
    destination: &Path,
    source: &Path,
    authority: &Path,
    authority_git: &Path,
) -> Result<()> {
    let parent = destination
        .parent()
        .context("destination must have a parent")?
        .canonicalize()?;
    let destination = parent.join(
        destination
            .file_name()
            .context("destination must name a directory")?,
    );
    for other in [source, authority, authority_git] {
        ensure!(
            !destination.starts_with(other) && !other.starts_with(&destination),
            "destination must be outside and disjoint from source and authority"
        );
    }
    ensure!(
        !source.starts_with(authority) && !authority.starts_with(source),
        "source workspace and authority paths must be disjoint"
    );
    Ok(())
}

fn random_owner() -> Result<String> {
    let file = tempfile::Builder::new()
        .rand_bytes(32)
        .prefix("mai-continuation-owner-")
        .tempfile()?;
    let name = file
        .path()
        .file_name()
        .context("random owner file has no name")?
        .to_string_lossy();
    Ok(name
        .trim_start_matches("mai-continuation-owner-")
        .to_owned())
}

fn bounded_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).chars().take(2048).collect()
}

fn write_marker(path: &Path, marker: &ContinuationMarker) -> Result<()> {
    let bytes = serde_json::to_vec(marker)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<()> {
    let _ = path;
    Ok(())
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
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command
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
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

fn git_value(path: &Path, args: &[&str]) -> Result<String> {
    let output = git_command()
        .arg("-C")
        .arg(external_git_path(path))
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "read-only source Git query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn git_bytes(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = git_command()
        .arg("-C")
        .arg(external_git_path(path))
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "read-only source Git query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn git_dir_command(path: &Path) -> Command {
    let mut command = git_command();
    command.arg("--git-dir").arg(external_git_path(path));
    command
}

fn git_dir_bytes(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = git_dir_command(path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "read-only authority Git query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn git_config_file(path: &Path, args: &[&str]) -> Result<std::process::Output> {
    Ok(git_command()
        .arg("config")
        .arg("--no-includes")
        .arg("--file")
        .arg(external_git_path(path))
        .args(args)
        .output()?)
}

fn reject_symlinks_under(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "source Git refs may not contain symlinks"
        );
        if metadata.is_dir() {
            reject_symlinks_under(&entry.path())?;
        }
    }
    Ok(())
}

fn git_dir_value(path: &Path, args: &[&str]) -> Result<String> {
    let output = git_dir_command(path).args(args).output()?;
    ensure!(
        output.status.success(),
        "read-only Git query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
