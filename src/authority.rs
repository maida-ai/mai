//! Authority initialization and evidence-bound startup reconciliation.
//!
//! Callers must hold the authority's engine lock before reconciliation. This
//! module does not accept submissions or publish candidates to trunk.
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    lifecycle::{GenerationId, Outcome, State},
    repository_scope::{external_git_path, validate_git_version, validate_source},
    state::{LandingRecord, StateStore, ValidationEvidence},
};

const MARKER: &str = ".mai-initializing";
const NOTES: &str = "refs/notes/mai-objects";
const DENY_RECEIVE: &str = "#!/bin/sh\necho 'mai receive is unavailable until registration support is enabled' >&2\nexit 1\n";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    format_version: u32,
    authority_id: String,
    baseline: String,
}

#[derive(Debug)]
pub struct Authority {
    root: PathBuf,
    config: Config,
}

struct InitLock(fs::File);
impl Drop for InitLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

impl Authority {
    /// Open required identity-bound durable leases. Missing state is never recreated.
    pub fn lease_store(&self) -> Result<crate::lease::LeaseStore> {
        crate::lease::LeaseStore::open(self.root.join(".mai/leases.db"), &self.config.authority_id)
    }

    /// Build in a private staging directory, then install into an absent or
    /// empty destination. An interrupted installation remains visibly incomplete.
    pub fn init(path: &Path, source: Option<&Path>) -> Result<Self> {
        validate_git_version()?;
        reject_symlink(path)?;
        // Resolve the destination before choosing its staging parent. In
        // particular, the lexical parent of `.` is not its filesystem parent;
        // staging there would make the destination nonempty ourselves.
        let destination = if path.exists() {
            path.canonicalize()?
        } else {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .canonicalize()
                .context("destination parent must already exist")?;
            parent.join(
                path.file_name()
                    .context("destination must name a directory")?,
            )
        };
        let path = destination.as_path();
        let validated = source.map(validate_source).transpose()?;
        if path.join(".mai/config.toml").exists() && !path.join(MARKER).exists() {
            let mut authority = Self::open(path)?;
            if let Some(source) = validated {
                ensure!(
                    source.head.as_deref() == Some(&authority.config.baseline),
                    "existing authority has a different import baseline; use a new destination"
                );
            }
            let _lock = authority.lock()?;
            authority.reconcile()?;
            return Ok(authority);
        }
        if path.exists() {
            ensure!(
                path.is_dir() && fs::read_dir(path)?.next().is_none(),
                "destination is not empty or initialization was interrupted; preserve it for inspection and initialize a new empty destination"
            );
        }
        let parent = path
            .parent()
            .context("destination must have a parent directory for staging")?;
        ensure!(parent.is_dir(), "destination parent must already exist");
        let stage = tempfile::Builder::new()
            .prefix(".mai-stage-")
            .tempdir_in(parent)?;
        let git_dir = stage.path().join("authority.git");
        if let Some(source) = validated {
            git(
                None,
                &[
                    "clone".into(),
                    "--bare".into(),
                    "--no-local".into(),
                    "--no-hardlinks".into(),
                    "--".into(),
                    external_git_path(&source.path),
                    external_git_path(&git_dir),
                ],
                None,
            )?;
        } else {
            git(
                None,
                &[
                    "init".into(),
                    "--bare".into(),
                    "--object-format=sha1".into(),
                    external_git_path(&git_dir),
                ],
                None,
            )?;
        }
        // The source can change while it is cloned. Qualify the actual retained copy.
        let staged = validate_source(&git_dir)?;
        let baseline = if let Some(head) = staged.head {
            head
        } else {
            let tree = git(Some(&git_dir), &["mktree"], Some(b""))?;
            git(
                Some(&git_dir),
                &[
                    "commit-tree",
                    tree.trim(),
                    "-m",
                    "mai initial baseline\n\nMai-Revision: r0",
                ],
                None,
            )?
            .trim()
            .to_owned()
        };
        git(
            Some(&git_dir),
            &["update-ref", "refs/heads/trunk", &baseline],
            None,
        )?;
        git(
            Some(&git_dir),
            &["symbolic-ref", "HEAD", "refs/heads/trunk"],
            None,
        )?;
        git(Some(&git_dir), &["config", "core.hooksPath", "hooks"], None)?;
        fs::create_dir_all(git_dir.join("hooks"))?;
        let hook = git_dir.join("hooks/pre-receive");
        fs::write(&hook, DENY_RECEIVE)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        }
        let local = stage.path().join(".mai");
        fs::create_dir(&local)?;
        let config = Config {
            format_version: 2,
            authority_id: stage
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_start_matches('.')
                .to_owned(),
            baseline,
        };
        {
            let mut state = StateStore::create(local.join("state.db"))?;
            state.set_authority_id(&config.authority_id)?;
            state.set_baseline(&config.baseline)?;
        }
        drop(crate::lease::LeaseStore::create(
            local.join("leases.db"),
            &config.authority_id,
        )?);
        fs::write(local.join("config.toml"), toml::to_string(&config)?)?;
        if !path.exists() {
            fs::create_dir(path)?;
        }
        // create_new reserves initialization against another initializer. Never
        // clean up an incomplete destination automatically or replace user data.
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join(MARKER))?;
        marker.write_all(b"Initialization incomplete. Preserve this directory and retry with a new empty destination.\n")?;
        marker.sync_all()?;
        ensure!(
            fs::read_dir(path)?.count() == 1,
            "destination changed during initialization; preserve it and use a new destination"
        );
        install_exclusive(&git_dir, &path.join("authority.git"))?;
        install_exclusive(&local, &path.join(".mai"))?;
        fs::remove_file(path.join(MARKER))?;
        Self::open(path)
    }

    /// Validate format and identity, without mutating acceptance state.
    pub fn open(path: &Path) -> Result<Self> {
        for component in [
            path.to_path_buf(),
            path.join("authority.git"),
            path.join(".mai"),
            path.join(".mai/config.toml"),
            path.join(".mai/state.db"),
            path.join(".mai/leases.db"),
        ] {
            reject_symlink(&component)?;
        }
        ensure!(
            !path.join(MARKER).exists(),
            "initialization incomplete; preserve the directory for inspection and use a new destination"
        );
        let config: Config = toml::from_str(
            &fs::read_to_string(path.join(".mai/config.toml"))
                .context("missing authority configuration; initialize a new destination")?,
        )?;
        ensure!(
            config.format_version == 2,
            "unsupported authority format; no conversion is supported"
        );
        let root = path.canonicalize()?;
        let state = StateStore::open(root.join(".mai/state.db"))?;
        ensure!(
            state.authority_id()?.as_deref() == Some(&config.authority_id),
            "authority identity does not match database"
        );
        ensure!(
            state.baseline()?.as_deref() == Some(&config.baseline),
            "import baseline does not match database"
        );
        let authority = Self { root, config };
        authority.lease_store()?;
        ensure!(
            authority
                .git(&["rev-parse", "--is-bare-repository"], None)?
                .trim()
                == "true",
            "authority.git must be a bare repository"
        );
        ensure!(
            authority
                .git(&["rev-parse", "--show-object-format"], None)?
                .trim()
                == "sha1",
            "unsupported authority object format"
        );
        authority.git(
            &[
                "cat-file",
                "-e",
                &format!("{}^{{commit}}", authority.config.baseline),
            ],
            None,
        )?;
        Ok(authority)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn git_dir(&self) -> PathBuf {
        self.root.join("authority.git")
    }
    pub fn state_path(&self) -> PathBuf {
        self.root.join(".mai/state.db")
    }

    fn lock(&self) -> Result<InitLock> {
        let path = self.root.join(".mai/engine.lock");
        reject_symlink(&path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        lock.try_lock_exclusive()
            .context("authority engine is active; stop it before repeating initialization")?;
        Ok(InitLock(lock))
    }

    /// Reconcile only exact, previously validated candidates. The caller holds
    /// the engine lock; Git and SQLite are intentionally separate writes.
    pub fn reconcile(&mut self) -> Result<()> {
        self.reconcile_servicing(|_| Ok(()))
    }

    pub(crate) fn reconcile_servicing(
        &mut self,
        mut service: impl FnMut(&mut Self) -> Result<()>,
    ) -> Result<()> {
        service(self)?;
        let mut state = StateStore::open(self.state_path())?;
        let lifecycle = state.load_lifecycle()?;
        for submission in lifecycle.submissions() {
            service(self)?;
            let id = submission.id();
            let reference = format!("refs/mai/changes/{}/{}", id.change_id, id.generation);
            ensure!(
                self.git(&["rev-parse", "--verify", &reference], None)?
                    .trim()
                    == submission.commit(),
                "retained submission reference disagrees with acknowledged generation"
            );
        }
        let all_evidence = state.evidence_all()?;
        let refs = self.git(
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/mai/candidates/",
            ],
            None,
        )?;
        let refs: BTreeMap<_, _> = refs
            .lines()
            .map(|line| {
                line.split_once(' ')
                    .context("invalid candidate ref listing")
            })
            .collect::<Result<_>>()?;
        let mut missing_candidates = Vec::new();
        for evidence in &all_evidence {
            service(self)?;
            let reference = format!(
                "refs/mai/candidates/{}/{}/{}",
                evidence.generation.change_id, evidence.generation.generation, evidence.attempt
            );
            // Missing candidates can be restored only from immutable expected
            // commit identity whose complete object and evidence still exist.
            self.verify_candidate(evidence)?;
            if let Some(existing) = refs.get(reference.as_str()) {
                ensure!(
                    *existing == evidence.candidate_commit,
                    "candidate reference differs from durable evidence"
                );
            } else {
                missing_candidates.push((reference, evidence.candidate_commit.clone()));
            }
        }
        let chain = self.git(&["rev-list", "--first-parent", "refs/heads/trunk"], None)?;
        let mut accepted = Vec::new();
        let mut found_baseline = false;
        for commit in chain.lines() {
            if commit == self.config.baseline {
                found_baseline = true;
                break;
            }
            accepted.push(commit.to_owned());
        }
        ensure!(
            found_baseline,
            "trunk does not descend from the recorded import baseline"
        );
        accepted.reverse();
        let previous_records = state.landings()?;
        for record in &previous_records {
            ensure!(
                accepted.contains(&record.commit),
                "database landing is absent from authoritative trunk; preserve state and investigate"
            );
        }
        let mut parent = self.config.baseline.clone();
        let mut verified = Vec::new();
        for (offset, commit) in accepted.iter().enumerate() {
            service(self)?;
            let trailers = self.trailers(commit)?;
            let generation: GenerationId =
                serde_json::from_str(required(&trailers, "Mai-Generation")?)?;
            let attempt: u64 = required(&trailers, "Mai-Attempt")?.parse()?;
            let revision: u64 = required(&trailers, "Mai-Revision")?
                .strip_prefix('r')
                .context("revision must use rN")?
                .parse()?;
            // Read and verify stored revisions; traversal never supplies a missing trailer.
            ensure!(
                revision == offset as u64 + 1,
                "nonconsecutive stored Mai-Revision"
            );
            let evidence = all_evidence
                .iter()
                .find(|e| e.generation == generation && e.attempt == attempt)
                .context("trunk commit has no exact generation/attempt evidence")?;
            ensure!(
                evidence.passed
                    && evidence.candidate_commit == *commit
                    && evidence.candidate_parent == parent,
                "trunk is not the exact durably passing candidate"
            );
            ensure!(
                required(&trailers, "Mai-Policy")? == evidence.trusted_policy,
                "trusted policy identity mismatch"
            );
            let submission = lifecycle
                .get(&generation)
                .context("unknown landed generation")?;
            ensure!(
                matches!(submission.state(), State::Validating { attempt: active } if *active == attempt)
                    || matches!(submission.state(), State::Terminal { outcome: Outcome::Landed { revision: recorded_revision, commit: recorded_commit } } if *recorded_revision == revision && recorded_commit == commit),
                "landing belongs to an interrupted, superseded or otherwise inactive attempt"
            );
            let note = self.landed_note(evidence, revision)?;
            verified.push((
                LandingRecord {
                    generation,
                    attempt,
                    commit: commit.clone(),
                    revision,
                },
                note,
            ));
            parent = commit.clone();
        }
        // Validate the entire ledger before updating any landing or derived note.
        if crate::receive::enabled(self)? {
            // Registration may have added records while the ledger was verified.
            // Their own admission already checked retained refs; include their notes
            // and never write the earlier lifecycle snapshot back over them.
            let current = state.load_lifecycle()?;
            for submission in current.submissions() {
                service(self)?;
                self.restore_received_note(submission)?;
            }
        }
        for (reference, commit) in missing_candidates {
            service(self)?;
            self.git(
                &[
                    "update-ref",
                    &reference,
                    &commit,
                    "0000000000000000000000000000000000000000",
                ],
                None,
            )?;
        }
        for (record, note) in verified {
            service(self)?;
            self.restore_note(&record.commit, note)?;
            state.record_landing(&record)?;
        }
        let mut lifecycle = state.load_lifecycle()?;
        let interrupted: Vec<_> = lifecycle
            .submissions()
            .filter_map(|submission| {
                if let State::Validating { attempt } = submission.state() {
                    Some((submission.id().clone(), *attempt))
                } else {
                    None
                }
            })
            .collect();
        if !interrupted.is_empty() {
            for (id, attempt) in interrupted {
                lifecycle.retry_attempt(&id, attempt)?;
            }
            state.save_lifecycle(&mut lifecycle)?;
        }
        state.rebuild_index()?;
        Ok(())
    }

    fn verify_candidate(&self, evidence: &ValidationEvidence) -> Result<()> {
        let identity = self.git(
            &["show", "-s", "--format=%T%n%P", &evidence.candidate_commit],
            None,
        )?;
        let mut lines = identity.lines();
        ensure!(
            lines.next() == Some(&evidence.candidate_tree)
                && lines.next() == Some(&evidence.candidate_parent),
            "candidate tree or sole parent differs from durable evidence"
        );
        Ok(())
    }

    fn trailers(&self, commit: &str) -> Result<BTreeMap<String, String>> {
        let message = self.git(&["show", "-s", "--format=%B", commit], None)?;
        let parsed = self.git(&["interpret-trailers", "--parse"], Some(message.as_bytes()))?;
        let mut result = BTreeMap::new();
        for line in parsed.lines() {
            if let Some((key, value)) = line.split_once(':') {
                if key.starts_with("Mai-") {
                    ensure!(
                        result
                            .insert(key.to_owned(), value.trim().to_owned())
                            .is_none(),
                        "duplicate authority trailer"
                    );
                }
            }
        }
        Ok(result)
    }

    fn landed_note(&self, evidence: &ValidationEvidence, revision: u64) -> Result<Value> {
        let bytes = git_bytes(
            Some(&self.git_dir()),
            &[
                "diff",
                "--name-status",
                "--no-renames",
                "-z",
                &evidence.candidate_parent,
                &evidence.candidate_tree,
                "--",
            ],
            None,
        )?;
        let fields: Vec<_> = bytes.split(|b| *b == 0).filter(|f| !f.is_empty()).collect();
        ensure!(fields.len() % 2 == 0, "malformed Git diff output");
        let mut files = Vec::new();
        for pair in fields.chunks_exact(2) {
            let operation = match pair[0] {
                b"A" => "added",
                b"D" => "deleted",
                b"M" | b"T" => "modified",
                _ => bail!("unsupported diff operation"),
            };
            files.push((pair[1].to_vec(), operation));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let units: Vec<_> = files
            .iter()
            .map(|(path, _)| file_unit(path))
            .collect::<Result<_>>()?;
        ensure!(
            units == evidence.manifest,
            "durable manifest does not match actual landed diff"
        );
        let entries: Vec<_> = files.iter().zip(&units).map(|((_,operation),unit)| json!({"unit":unit,"operation":operation,"granularity":"file","parser":null,"fallback_reason":"mvp-file-only"})).collect();
        Ok(
            json!({"phase":"landed","generation":evidence.generation,"submitted_commit":evidence.submitted_commit,"base":evidence.candidate_parent,"tree":evidence.candidate_tree,"landed_commit":evidence.candidate_commit,"parent":evidence.candidate_parent,"revision":revision,"entries":entries}),
        )
    }

    pub(crate) fn restore_received_note(
        &self,
        submission: &crate::lifecycle::Submission,
    ) -> Result<()> {
        let parent = self.git(&["show", "-s", "--format=%P", submission.commit()], None)?;
        let parents: Vec<_> = parent.split_whitespace().collect();
        ensure!(
            parents.len() == 1,
            "received commit must have exactly one parent"
        );
        let manifest = crate::manifest::compute(&self.git_dir(), parents[0], submission.commit())?;
        let note = json!({"phase":"received", "generation":submission.id(), "submitted_commit":submission.commit(), "base":parents[0], "tree":manifest.result_tree, "entries":manifest.entries});
        self.restore_note(submission.commit(), note)
    }

    fn restore_note(&self, commit: &str, record: Value) -> Result<()> {
        let listing = self.git(&["notes", "--ref", NOTES, "list"], None)?;
        let current = listing.lines().find_map(|line| {
            line.split_once(' ')
                .filter(|(_, object)| *object == commit)
                .map(|(note, _)| note)
        });
        let mut envelope = match current {
            Some(note) => {
                serde_json::from_str::<Value>(&self.git(&["cat-file", "blob", note], None)?)
                    .context("invalid existing manifest note")?
            }
            None => json!({"format_version":1,"records":[]}),
        };
        ensure!(
            envelope["format_version"] == 1,
            "unsupported manifest note version"
        );
        let records = envelope["records"]
            .as_array_mut()
            .context("manifest note records are malformed")?;
        let mut matched = false;
        let mut associations = BTreeSet::new();
        for previous in records.iter() {
            validate_note_record(previous, commit)?;
            ensure!(
                associations.insert((
                    previous["phase"].to_string(),
                    previous["generation"].to_string()
                )),
                "duplicate manifest phase/generation association"
            );
            if previous["phase"] == record["phase"]
                && previous["generation"] == record["generation"]
            {
                ensure!(!matched, "duplicate manifest association");
                ensure!(
                    *previous == record,
                    "existing manifest contradicts durable evidence"
                );
                matched = true;
            }
        }
        if matched {
            return Ok(());
        }
        records.push(record);
        self.git(
            &["notes", "--ref", NOTES, "add", "-f", "-F", "-", commit],
            Some(&serde_json::to_vec(&envelope)?),
        )?;
        Ok(())
    }

    pub(crate) fn git_bytes(&self, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
        git_bytes(Some(&self.git_dir()), args, input)
    }

    pub(crate) fn git(&self, args: &[&str], input: Option<&[u8]>) -> Result<String> {
        git(Some(&self.git_dir()), args, input)
    }
}

fn required<'a>(trailers: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    trailers
        .get(name)
        .map(String::as_str)
        .with_context(|| format!("missing {name} trailer"))
}
fn validate_note_record(record: &Value, object: &str) -> Result<()> {
    ensure!(record.is_object(), "manifest record must be an object");
    let generation: GenerationId = serde_json::from_value(record["generation"].clone())
        .context("invalid manifest generation")?;
    ensure!(
        generation.generation > 0
            && !generation.change_id.is_empty()
            && generation.change_id.len() <= 128
            && generation
                .change_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "invalid manifest generation identity"
    );
    for key in ["submitted_commit", "base", "tree"] {
        ensure!(
            record[key].as_str().is_some_and(valid_oid),
            "invalid manifest object identity"
        );
    }
    match record["phase"].as_str() {
        Some("received") => ensure!(
            record["submitted_commit"] == object,
            "received manifest is attached to the wrong object"
        ),
        Some("landed") => {
            ensure!(
                record["landed_commit"] == object
                    && record["parent"] == record["base"]
                    && record["revision"].as_u64().is_some_and(|r| r > 0),
                "landed manifest association is malformed"
            );
        }
        _ => bail!("unknown manifest phase"),
    }
    let entries = record["entries"]
        .as_array()
        .context("manifest entries must be an array")?;
    let mut last_path: Option<Vec<u8>> = None;
    for entry in entries {
        let path = crate::state::canonical_path(
            entry["unit"]
                .as_str()
                .context("manifest unit must be a string")?,
        )?;
        ensure!(
            last_path.as_ref().is_none_or(|previous| previous < &path),
            "manifest entries must be unique and sorted by raw path bytes"
        );
        last_path = Some(path);
        ensure!(
            entry["unit"]
                .as_str()
                .is_some_and(|unit| unit.starts_with("file:"))
                && matches!(
                    entry["operation"].as_str(),
                    Some("added" | "modified" | "deleted")
                )
                && entry["granularity"] == "file"
                && entry["parser"].is_null()
                && entry["fallback_reason"] == "mvp-file-only",
            "malformed file manifest entry"
        );
    }
    Ok(())
}
fn valid_oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value.bytes().any(|b| b != b'0')
}
fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => ensure!(
            !metadata.file_type().is_symlink(),
            "refusing symlink path {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn install_exclusive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination).context(
        "installation destination changed; preserve incomplete initialization and retry elsewhere",
    )?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            install_exclusive(&entry.path(), &target)?;
        } else {
            ensure!(
                kind.is_file(),
                "unexpected nonregular staged authority file"
            );
            let mut input = fs::File::open(entry.path())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            fs::set_permissions(&target, input.metadata()?.permissions())?;
        }
    }
    Ok(())
}
fn git<S: AsRef<OsStr>>(path: Option<&Path>, args: &[S], input: Option<&[u8]>) -> Result<String> {
    String::from_utf8(git_bytes(path, args, input)?).context("unexpected non-UTF-8 Git metadata")
}
fn git_bytes<S: AsRef<OsStr>>(
    path: Option<&Path>,
    args: &[S],
    input: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "mai")
        .env("GIT_AUTHOR_EMAIL", "mai@localhost")
        .env("GIT_COMMITTER_NAME", "mai")
        .env("GIT_COMMITTER_EMAIL", "mai@localhost")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.file.allow=always",
        ]);
    if let Some(path) = path {
        command.arg("--git-dir").arg(external_git_path(path));
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input)?;
    }
    drop(child.stdin.take());
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.first()
            .map(|arg| arg.as_ref().to_string_lossy())
            .unwrap_or_default(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn file_unit(path: &[u8]) -> Result<String> {
    ensure!(
        !path.is_empty()
            && !path.contains(&0)
            && path
                .split(|b| *b == b'/')
                .all(|part| !part.is_empty() && part != b"." && part != b".."),
        "invalid repository-relative path"
    );
    let mut unit = String::from("file:");
    for byte in path {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(byte) {
            unit.push(*byte as char);
        } else {
            use std::fmt::Write;
            write!(&mut unit, "%{byte:02X}")?;
        }
    }
    Ok(unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_install_never_replaces_concurrent_destination() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("stage");
        let destination = temp.path().join("authority.git");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("HEAD"), b"staged").unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("HEAD"), b"user").unwrap();
        assert!(install_exclusive(&source, &destination).is_err());
        assert_eq!(fs::read(destination.join("HEAD")).unwrap(), b"user");
        assert_eq!(fs::read(source.join("HEAD")).unwrap(), b"staged");
    }

    #[test]
    fn file_units_encode_raw_bytes_without_aliases() {
        assert_eq!(
            file_unit(b"a %20\\:\xff\n").unwrap(),
            "file:a%20%2520%5C%3A%FF%0A"
        );
        for path in [
            b"".as_slice(),
            b"/x",
            b"x/",
            b"x//y",
            b"./x",
            b"../x",
            b"x\0y",
        ] {
            assert!(file_unit(path).is_err());
        }
    }
}
