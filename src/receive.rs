//! Explicitly enabled, trusted-local Git admission. Registration is not landing.
//! Call mutation functions only while holding the engine's exclusive ownership lock.
use crate::{
    authority::Authority,
    lifecycle::{GenerationId, Submission},
    state::StateStore,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

const ZERO: &str = "0000000000000000000000000000000000000000";
const MAX_PACKET: usize = 1024;
const HOOK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    format_version: u32,
    max_input_bytes: u64,
    max_file_bytes: u64,
    max_changed_files: u64,
}

fn configuration(authority: &Authority) -> Result<Option<Config>> {
    let path = authority.root().join(".mai/receive.json");
    if !path.try_exists()? {
        return Ok(None);
    }
    ensure!(
        fs::symlink_metadata(&path)?.is_file(),
        "receive configuration must be a regular file"
    );
    let config: Config = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        config.format_version == 1
            && config.max_input_bytes > 0
            && config.max_file_bytes > 0
            && config.max_changed_files > 0,
        "invalid receive configuration"
    );
    Ok(Some(config))
}
pub fn enabled(authority: &Authority) -> Result<bool> {
    Ok(configuration(authority)?.is_some())
}

/// The caller selects a pack-size limit; there is no implicit admission default.
pub fn enable(
    authority: &Authority,
    max_input_bytes: u64,
    max_file_bytes: u64,
    max_changed_files: u64,
) -> Result<()> {
    ensure!(
        max_input_bytes > 0
            && max_input_bytes <= i64::MAX as u64
            && max_file_bytes > 0
            && max_changed_files > 0,
        "limits must be positive; max-input-bytes must fit a signed 64-bit integer"
    );
    let executable = std::env::current_exe()?;
    let quote = |path: &Path| -> Result<String> {
        let text = path
            .to_str()
            .context("hook executable and authority paths must be UTF-8")?;
        ensure!(
            !text.contains(['\n', '\r']),
            "hook paths cannot contain newlines"
        );
        let text = if cfg!(windows) {
            text.replace('\\', "/")
        } else {
            text.to_owned()
        };
        Ok(format!("'{}'", text.replace('\'', "'\\''")))
    };
    let executable = quote(&executable)?;
    let root = quote(authority.root())?;
    // Route every namespace through proc-receive; no ordinary ref update falls through.
    authority.git(
        &[
            "config",
            "--replace-all",
            "receive.procReceiveRefs",
            "refs/",
        ],
        None,
    )?;
    authority.git(&["config", "receive.advertisePushOptions", "true"], None)?;
    authority.git(&["config", "receive.fsckObjects", "true"], None)?;
    authority.git(
        &[
            "config",
            "receive.maxInputSize",
            &max_input_bytes.to_string(),
        ],
        None,
    )?;
    for (name, command) in [
        ("proc-receive", "receive-hook"),
        ("pre-receive", "receive-preflight"),
    ] {
        let path = authority.git_dir().join("hooks").join(name);
        ensure!(
            !fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file()),
            "hook must be a regular file"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(authority.git_dir().join("hooks"))?;
        write!(temporary, "#!/bin/sh\nexec {executable} {command} {root}\n")?;
        temporary.as_file().sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o755))?;
        }
        temporary.persist(&path)?;
    }
    let mut config = tempfile::NamedTempFile::new_in(authority.root().join(".mai"))?;
    serde_json::to_writer(
        &mut config,
        &Config {
            format_version: 1,
            max_input_bytes,
            max_file_bytes,
            max_changed_files,
        },
    )?;
    config.as_file().sync_all()?;
    config.persist(authority.root().join(".mai/receive.json"))?;
    Ok(())
}

fn valid_agent(agent: &str) -> bool {
    !agent.is_empty()
        && agent.len() <= 64
        && agent
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}
fn valid_oid(oid: &str) -> bool {
    oid.len() == 40
        && oid != ZERO
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn register(authority: &Authority, agent: &str, commit: &str) -> Result<Submission> {
    register_with_checkpoints(authority, agent, commit, |_| {})
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Checkpoint {
    Retained,
    Noted,
    Stored,
}
fn register_with_checkpoints(
    authority: &Authority,
    agent: &str,
    commit: &str,
    checkpoint: impl Fn(Checkpoint),
) -> Result<Submission> {
    let limits = configuration(authority)?.context("receive admission is disabled")?;
    ensure!(
        valid_agent(agent),
        "agent must contain 1-64 ASCII letters, digits, hyphens or underscores"
    );
    ensure!(
        valid_oid(commit),
        "expected a nonzero lowercase SHA-1 commit"
    );
    let mut store = StateStore::open(authority.state_path())?;
    let mut lifecycle = store.load_lifecycle()?;
    if let Some(previous) = lifecycle.submissions().find(|s| s.commit() == commit) {
        ensure!(
            previous.agent() == agent,
            "commit already belongs to another declared agent"
        );
        let reference = retained_ref(previous.id());
        ensure!(
            authority
                .git(&["rev-parse", "--verify", &reference], None)?
                .trim()
                == commit,
            "retained identity differs from durable submission"
        );
        authority.restore_received_note(previous)?;
        return Ok(previous.clone());
    }
    ensure!(
        authority.git(&["cat-file", "-t", commit], None)?.trim() == "commit",
        "submission must be a commit object"
    );
    let identity = authority.git(&["show", "-s", "--format=%P", commit], None)?;
    let parents: Vec<_> = identity.split_whitespace().collect();
    ensure!(
        parents.len() == 1,
        "receive currently requires a single-parent commit"
    );
    let parent = parents[0];
    let dependencies =
        if let Some(submitted) = lifecycle.submissions().find(|s| s.commit() == parent) {
            vec![submitted.id().clone()]
        } else {
            authority
                .git(
                    &["merge-base", "--is-ancestor", parent, "refs/heads/trunk"],
                    None,
                )
                .context(
                    "submit each pending parent first; parent is neither registered nor on trunk",
                )?;
            vec![]
        };
    check_scope(authority, parent, commit, &limits)?;
    crate::repository_scope::validate_commit(&authority.git_dir(), commit)?;
    let id = lifecycle.register(&format!("{agent}-{commit}"), agent, commit, dependencies)?;
    let reference = retained_ref(&id);
    // Git and SQLite are separate durable writes. An orphan exact ref is safe:
    // retain it indefinitely and complete registration only on an identical retry.
    let existing = authority.git(
        &["for-each-ref", "--format=%(objectname)", &reference],
        None,
    )?;
    if existing.is_empty() {
        authority.git(&["update-ref", &reference, commit, ZERO], None)?;
    } else {
        ensure!(
            existing.trim() == commit,
            "immutable submission reference collision"
        );
    }
    checkpoint(Checkpoint::Retained);
    authority.restore_received_note(
        lifecycle
            .get(&id)
            .context("registered submission missing")?,
    )?;
    checkpoint(Checkpoint::Noted);
    store.save_lifecycle(&mut lifecycle)?;
    checkpoint(Checkpoint::Stored);
    Ok(lifecycle
        .get(&id)
        .context("registered submission missing")?
        .clone())
}

fn check_scope(authority: &Authority, parent: &str, commit: &str, limits: &Config) -> Result<()> {
    let diff = authority.git_bytes(
        &[
            "diff",
            "--raw",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
            "--no-abbrev",
            "--no-renames",
            "-z",
            parent,
            commit,
            "--",
        ],
        None,
    )?;
    let parts: Vec<_> = diff.split(|b| *b == 0).filter(|v| !v.is_empty()).collect();
    ensure!(parts.len() % 2 == 0, "malformed receive diff");
    ensure!(
        (parts.len() / 2) as u64 <= limits.max_changed_files,
        "changed-file limit exceeded"
    );
    for pair in parts.chunks_exact(2) {
        ensure!(
            !pair[1]
                .split(|b| *b == b'/')
                .any(|p| p == b"node_modules" || p == b".env" || p.starts_with(b".env.")),
            "blocked receive path (.env, .env.* or node_modules)"
        );
        let metadata = std::str::from_utf8(pair[0])?;
        let fields: Vec<_> = metadata.split(' ').collect();
        ensure!(fields.len() == 5, "malformed receive diff metadata");
        if fields[3] != ZERO {
            let size: u64 = authority
                .git(&["cat-file", "-s", fields[3]], None)?
                .trim()
                .parse()?;
            ensure!(
                size <= limits.max_file_bytes,
                "per-file byte limit exceeded"
            );
        }
    }
    Ok(())
}

pub fn retained_ref(id: &GenerationId) -> String {
    format!("refs/mai/changes/{}/{}", id.change_id, id.generation)
}
pub fn query(authority: &Authority, id: &GenerationId) -> Result<Submission> {
    StateStore::open(authority.state_path())?
        .load_lifecycle()?
        .get(id)
        .cloned()
        .context("unknown submission generation")
}

fn packet(input: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let length = usize::from_str_radix(std::str::from_utf8(&header)?, 16)?;
    if length == 0 {
        return Ok(None);
    }
    ensure!(
        (4..=MAX_PACKET).contains(&length),
        "invalid or oversized hook packet"
    );
    let mut bytes = vec![0; length - 4];
    input.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}
fn packets(input: &mut impl Read, limit: usize) -> Result<Vec<String>> {
    let mut rows = Vec::new();
    while let Some(bytes) = packet(input)? {
        ensure!(rows.len() < limit, "too many hook packets");
        rows.push(String::from_utf8(bytes)?.trim_end_matches('\n').to_owned());
    }
    Ok(rows)
}
fn send(output: &mut impl Write, value: &str) -> Result<()> {
    write!(output, "{:04x}{value}", value.len() + 4)?;
    Ok(())
}
fn flush(output: &mut impl Write) -> Result<()> {
    output.write_all(b"0000")?;
    output.flush()?;
    Ok(())
}
fn command(row: &str) -> Result<&str> {
    let fields: Vec<_> = row.split(' ').collect();
    ensure!(
        fields.len() == 3
            && fields[0] == ZERO
            && valid_oid(fields[1])
            && fields[2] == "refs/for/main",
        "only a new non-deletion refs/for/main submission is allowed; drafts and direct ref writes are unavailable"
    );
    Ok(fields[1])
}

/// Bound a stalled or malformed hook client without making the engine wait on stdin.
pub fn hook(root: &Path, preflight: bool) -> Result<()> {
    let root = root.to_owned();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(run_hook(&root, preflight));
    });
    rx.recv_timeout(HOOK_TIMEOUT)
        .context("receive hook timed out")?
}
fn run_hook(root: &Path, preflight: bool) -> Result<()> {
    let mut input = std::io::stdin().lock();
    if preflight {
        let mut bytes = Vec::new();
        (&mut input)
            .take((MAX_PACKET + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_PACKET, "receive command exceeds limit");
        let text = std::str::from_utf8(&bytes)?;
        let rows: Vec<_> = text.lines().collect();
        ensure!(
            rows.len() == 1,
            "exactly one submission per push is supported"
        );
        command(rows[0])?;
        return Ok(());
    }
    let mut output = std::io::stdout().lock();
    let versions = packets(&mut input, 1)?;
    ensure!(
        versions.len() == 1 && versions[0].split('\0').next() == Some("version=1"),
        "unsupported proc-receive version"
    );
    let options = versions[0]
        .split('\0')
        .nth(1)
        .is_some_and(|s| s.split(' ').any(|s| s == "push-options"));
    send(
        &mut output,
        if options {
            "version=1\0push-options"
        } else {
            "version=1"
        },
    )?;
    flush(&mut output)?;
    let rows = packets(&mut input, 1)?;
    ensure!(
        rows.len() == 1,
        "exactly one submission per push is supported"
    );
    let commit = command(&rows[0])?;
    let opts = if options {
        packets(&mut input, 1)?
    } else {
        vec![]
    };
    let result = (|| {
        ensure!(
            opts.len() == 1,
            "provide exactly one push option: -o agent=NAME"
        );
        let agent = opts[0]
            .strip_prefix("agent=")
            .context("only agent=NAME push option is supported")?;
        crate::engine::register(root, agent, commit)
    })();
    match result {
        Ok(submission) => {
            eprintln!(
                "mai received {} generation {}; not landed",
                submission.id().change_id,
                submission.id().generation
            );
            send(&mut output, "ok refs/for/main")?;
            send(
                &mut output,
                &format!("option refname {}", retained_ref(submission.id())),
            )?;
            send(&mut output, &format!("option new-oid {commit}"))?;
        }
        Err(error) => {
            eprintln!(
                "mai registration failed: {error:#}; retry the identical commit if the outcome is uncertain"
            );
            send(
                &mut output,
                "ng refs/for/main registration failed; inspect remote diagnostics and retry identical commit",
            )?;
        }
    }
    flush(&mut output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_packets_and_namespace_are_fail_closed() {
        for bytes in [
            b"0001".as_slice(),
            b"ffff",
            b"zzzz",
            b"0008x",
            b"000400040000",
        ] {
            assert!(packets(&mut &*bytes, 1).is_err());
        }
        assert_eq!(packets(&mut b"0005x0000".as_slice(), 1).unwrap(), vec!["x"]);
        let oid = "1".repeat(40);
        assert_eq!(
            command(&format!("{ZERO} {oid} refs/for/main")).unwrap(),
            oid
        );
        for row in [
            format!("{ZERO} {oid} refs/heads/trunk"),
            format!("{ZERO} {ZERO} refs/for/main"),
            format!("{oid} {oid} refs/for/main"),
            format!("{ZERO} {oid} refs/drafts/alice"),
            format!("{ZERO} {oid} refs/for/main extra"),
        ] {
            assert!(command(&row).is_err());
        }
        for agent in ["", "../alice", "alice/bob", "a b", "a\n", &"x".repeat(65)] {
            assert!(!valid_agent(agent));
        }
    }
    #[test]
    #[ignore = "separate-process receive interruption worker"]
    fn crash_worker() {
        use fs2::FileExt;
        let root = std::path::PathBuf::from(std::env::var_os("MAI_TEST_RECEIVE_ROOT").unwrap());
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(".mai/engine.lock"))
            .unwrap();
        lock.try_lock_exclusive().unwrap();
        let authority = Authority::open(&root).unwrap();
        let commit = std::env::var("MAI_TEST_RECEIVE_COMMIT").unwrap();
        let stage = std::env::var("MAI_TEST_RECEIVE_STAGE").unwrap();
        register_with_checkpoints(&authority, "alice", &commit, |point| {
            if format!("{point:?}") == stage {
                fs::write(root.join(".mai/receive-test-ready"), "ready").unwrap();
                std::thread::sleep(Duration::from_secs(30));
                panic!("parent failed to kill checkpoint worker");
            }
        })
        .unwrap();
    }

    #[test]
    fn killed_receive_at_each_durable_boundary_retries_exactly_once() {
        use fs2::FileExt;
        use std::{
            process::{Command, Stdio},
            time::Instant,
        };
        for stage in ["Retained", "Noted", "Stored"] {
            let temp = tempfile::tempdir().unwrap();
            let mut authority = Authority::init(&temp.path().join("authority"), None).unwrap();
            enable(&authority, 1_000_000, 100_000, 10).unwrap();
            let base = authority.git(&["rev-parse", "trunk"], None).unwrap();
            let tree = authority.git(&["rev-parse", "trunk^{tree}"], None).unwrap();
            let commit = authority
                .git(
                    &[
                        "commit-tree",
                        tree.trim(),
                        "-p",
                        base.trim(),
                        "-m",
                        "submitted",
                    ],
                    None,
                )
                .unwrap()
                .trim()
                .to_owned();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "receive::tests::crash_worker",
                    "--ignored",
                    "--nocapture",
                ])
                .env("MAI_TEST_RECEIVE_ROOT", authority.root())
                .env("MAI_TEST_RECEIVE_COMMIT", &commit)
                .env("MAI_TEST_RECEIVE_STAGE", stage)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !authority.root().join(".mai/receive-test-ready").exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "worker exited before {stage}"
                );
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("worker timed out at {stage}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            child.wait().unwrap();
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(authority.root().join(".mai/engine.lock"))
                .unwrap();
            lock.try_lock_exclusive().unwrap();
            let count = StateStore::open(authority.state_path())
                .unwrap()
                .load_lifecycle()
                .unwrap()
                .submissions()
                .count();
            assert_eq!(count, usize::from(stage == "Stored"), "{stage}");
            authority.reconcile().unwrap();
            let first = register(&authority, "alice", &commit).unwrap();
            let retry = register(&authority, "alice", &commit).unwrap();
            assert_eq!(first, retry);
            assert_eq!(
                StateStore::open(authority.state_path())
                    .unwrap()
                    .load_lifecycle()
                    .unwrap()
                    .submissions()
                    .count(),
                1
            );
            let note = authority
                .git(&["notes", "--ref", "mai-objects", "show", &commit], None)
                .unwrap();
            let note: serde_json::Value = serde_json::from_str(&note).unwrap();
            assert_eq!(note["records"].as_array().unwrap().len(), 1);
            assert_eq!(authority.git(&["rev-parse", "trunk"], None).unwrap(), base);
            FileExt::unlock(&lock).unwrap();
        }
    }
}
