//! Disposable protocol experiment; deliberately not a product implementation.
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const POINTS: &[&str] = &[
    "registration_in_tx",
    "registration",
    "change_ref",
    "queue_in_tx",
    "queue",
    "ack",
    "candidate",
    "evidence",
    "before_cas",
    "after_cas",
    "manifest_note",
    "manifest_index",
    "terminal",
    "grant",
    "revoke",
    "expiry",
    "grace_notice",
    "grace_transfer",
    "attempt_preemption",
    "fix_window",
    "full_confirmation",
    "before_revert",
    "after_revert",
    "recovery_terminal",
    "dependent_rejection",
    "dependent_requeue",
];
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root.join("repo"))
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_CONFIG_COUNT")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "core.autocrlf=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=disabled-hooks",
        ])
        .args(args)
        .env("GIT_AUTHOR_NAME", "S7")
        .env("GIT_AUTHOR_EMAIL", "s7@example.invalid")
        .env("GIT_COMMITTER_NAME", "S7")
        .env("GIT_COMMITTER_EMAIL", "s7@example.invalid")
        .env("GIT_AUTHOR_DATE", "2001-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2001-01-01T00:00:00Z")
        .output()?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("commands.log"))?;
    writeln!(
        log,
        "git {args:?}: status={} stdout={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stdout).trim(),
        String::from_utf8_lossy(&out.stderr).trim()
    )?;
    ensure!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8(out.stdout)?.trim().into())
}
fn db(root: &Path) -> Result<Connection> {
    let c = Connection::open(root.join("state.sqlite"))?;
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY,v TEXT NOT NULL); CREATE TABLE IF NOT EXISTS outcomes(identity TEXT PRIMARY KEY, outcome TEXT NOT NULL);")?;
    Ok(c)
}
fn get(c: &Connection, k: &str) -> Result<Option<String>> {
    Ok(c.query_row("SELECT v FROM kv WHERE k=?", [k], |r| r.get(0))
        .optional()?)
}
fn need(c: &Connection, k: &str) -> Result<String> {
    get(c, k)?.with_context(|| format!("missing {k}"))
}
fn put(c: &Connection, k: &str, v: &str) -> Result<()> {
    c.execute(
        "INSERT INTO kv VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
        params![k, v],
    )?;
    Ok(())
}
fn once(c: &Connection, k: &str, v: &str) -> Result<()> {
    c.execute("INSERT OR IGNORE INTO kv VALUES(?1,?2)", params![k, v])?;
    Ok(())
}
fn checkpoint(root: &Path, selected: &str, name: &str) -> Result<()> {
    if selected == name {
        let mut f = fs::File::create(root.join("ready"))?;
        f.write_all(name.as_bytes())?;
        f.sync_all()?;
        loop {
            thread::sleep(Duration::from_millis(50));
        }
    }
    Ok(())
}
fn init(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("repo"))?;
    fs::write(root.join("expected-incident"), "20")?;
    git(root, &["init", "--quiet"])?;
    fs::write(root.join("repo/file.txt"), "baseline\n")?;
    git(root, &["add", "file.txt"])?;
    git(root, &["commit", "-qm", "baseline\n\nMai-Revision: r0"])?;
    let earlier = git(root, &["rev-parse", "HEAD"])?;
    git(root, &["update-ref", "refs/changes/same/1", &earlier])?;
    git(
        root,
        &[
            "commit",
            "--amend",
            "-qm",
            &format!("baseline\n\nChange-Id: same\nSubmission: same:1:{earlier}\nMai-Revision: r0"),
        ],
    )?;
    let base = git(root, &["rev-parse", "HEAD"])?;
    git(root, &["update-ref", "refs/heads/trunk", &base])?;
    fs::write(root.join("repo/file.txt"), "submission\n")?;
    git(root, &["add", "file.txt"])?;
    git(root, &["commit", "-qm", "submitted\n\nChange-Id: same"])?;
    let submitted = git(root, &["rev-parse", "HEAD"])?;
    git(root, &["update-ref", "refs/drafts/alice", &submitted])?;
    let c = db(root)?;
    put(&c, "base", &base)?;
    put(&c, "submitted", &submitted)?;
    put(&c, "identity", &format!("same:2:{submitted}"))?;
    // A same-ID earlier generation is present on an unrelated branch: never acceptance evidence.
    let tree = git(root, &["rev-parse", &format!("{submitted}^{{tree}}")])?;
    let fake = git(
        root,
        &[
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-m",
            &format!("decoy\n\nChange-Id: same\nSubmission: same:2:{submitted}\nGeneration: 2"),
        ],
    )?;
    git(root, &["update-ref", "refs/heads/decoy", &fake])?;
    Ok(())
}
fn terminal(c: &Connection, id: &str, outcome: &str) -> Result<()> {
    c.execute(
        "INSERT OR IGNORE INTO outcomes VALUES(?1,?2)",
        params![id, outcome],
    )?;
    let old: String = c.query_row("SELECT outcome FROM outcomes WHERE identity=?", [id], |r| {
        r.get(0)
    })?;
    ensure!(old == outcome, "terminal outcome changed");
    Ok(())
}
fn accepted(root: &Path, id: &str) -> Result<Option<String>> {
    for oid in git(root, &["rev-list", "--first-parent", "refs/heads/trunk"])?.lines() {
        let message = git(root, &["show", "-s", "--format=%B", oid])?;
        if message.lines().any(|l| l == format!("Submission: {id}")) {
            return Ok(Some(oid.into()));
        }
    }
    Ok(None)
}
fn worker(root: &Path, selected: &str) -> Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("engine.lock"))?;
    lock.try_lock_exclusive().context("engine lock held")?;
    let c = db(root)?;
    let base = need(&c, "base")?;
    let submitted = need(&c, "submitted")?;
    let id = need(&c, "identity")?;
    c.execute_batch("BEGIN IMMEDIATE")?;
    once(&c, "registered", &id)?;
    checkpoint(root, selected, "registration_in_tx")?;
    c.execute_batch("COMMIT")?;
    checkpoint(root, selected, "registration")?;
    git(root, &["update-ref", "refs/changes/same/2", &submitted])?;
    checkpoint(root, selected, "change_ref")?;
    c.execute_batch("BEGIN IMMEDIATE")?;
    once(&c, "queued", &id)?;
    checkpoint(root, selected, "queue_in_tx")?;
    c.execute_batch("COMMIT")?;
    checkpoint(root, selected, "queue")?;
    // Receipt is persisted only after durable reference and queue registration.
    once(&c, "ack", &submitted)?;
    checkpoint(root, selected, "ack")?;
    let tree = git(root, &["rev-parse", &format!("{submitted}^{{tree}}")])?;
    let candidate = git(
        root,
        &[
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-m",
            &format!("land\n\nSubmission: {id}\nMai-Revision: r1"),
        ],
    )?;
    git(root, &["update-ref", "refs/candidates/same/2", &candidate])?;
    put(&c, "candidate", &candidate)?;
    checkpoint(root, selected, "candidate")?;
    // Real immutable-tree check; this is a tiny synthetic gate, not a product validator.
    ensure!(
        git(root, &["show", &format!("{candidate}:file.txt")])? == "submission",
        "gate failed"
    );
    let policy = git(root, &["rev-parse", &format!("{base}^{{tree}}")])?;
    let evidence = format!("{candidate}|{tree}|{base}|{policy}|attempt=1|PASS");
    once(&c, "evidence", &evidence)?;
    ensure!(need(&c, "evidence")? == evidence, "stale evidence");
    checkpoint(root, selected, "evidence")?;
    if let Some(landed) = accepted(root, &id)? {
        ensure!(landed == candidate, "identity mismatch");
    } else {
        checkpoint(root, selected, "before_cas")?;
        git(root, &["update-ref", "refs/heads/trunk", &candidate, &base])?;
        checkpoint(root, selected, "after_cas")?;
    }
    let manifest = git(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-r",
            &base,
            &candidate,
        ],
    )?;
    let manifest = format!("file-fallback:no-parser\n{manifest}");
    git(
        root,
        &[
            "notes",
            "--ref=manifests",
            "add",
            "-f",
            "-m",
            &manifest,
            &candidate,
        ],
    )?;
    checkpoint(root, selected, "manifest_note")?;
    put(&c, "manifest", &manifest)?;
    checkpoint(root, selected, "manifest_index")?;
    terminal(&c, &id, "landed")?;
    checkpoint(root, selected, "terminal")?;
    // Durable event timestamps use an explicitly synthetic monotonic logical clock.
    once(&c, "grant", "high:expires=20")?;
    checkpoint(root, selected, "grant")?;
    once(&c, "revoke", "high:at=12")?;
    checkpoint(root, selected, "revoke")?;
    once(&c, "expiry", "at=20:normal")?;
    checkpoint(root, selected, "expiry")?;
    once(&c, "grace_notice", "owner=alice:deadline=30")?;
    checkpoint(root, selected, "grace_notice")?;
    once(&c, "grace_transfer", "at=30:owner=bob")?;
    checkpoint(root, selected, "grace_transfer")?;
    once(&c, "attempt_preemption", "attempt=1:cancelled:at=31")?;
    checkpoint(root, selected, "attempt_preemption")?;
    // Explicit confirmed culprit evidence; deadline must survive restart.
    ensure!(
        git(root, &["show", &format!("{base}:file.txt")])? == "baseline",
        "before evidence"
    );
    once(&c, "culprit", &candidate)?;
    let now = env::var("S7_LOGICAL_TIME").unwrap_or_else(|_| "20".into());
    once(&c, "incident_start", &now)?;
    let start: u64 = need(&c, "incident_start")?.parse()?;
    once(&c, "fix_deadline", &(start + 100).to_string())?;
    checkpoint(root, selected, "fix_window")?;
    once(&c, "full_confirmation", "failed:at=99")?;
    checkpoint(root, selected, "full_confirmation")?;
    let old_tree = git(root, &["rev-parse", &format!("{base}^{{tree}}")])?;
    let revert = git(
        root,
        &[
            "commit-tree",
            &old_tree,
            "-p",
            &candidate,
            "-m",
            &format!("compensate\n\nCompensates: {id}\nMai-Revision: r2"),
        ],
    )?;
    // Synthetic backstop gate checks the exact proposed final tree before CAS.
    ensure!(
        git(root, &["show", &format!("{revert}:file.txt")])? == "baseline",
        "revert gate"
    );
    once(
        &c,
        "revert_evidence",
        &format!("{revert}|{old_tree}|{candidate}|PASS"),
    )?;
    let trunk = git(root, &["rev-parse", "refs/heads/trunk"])?;
    if trunk != revert {
        ensure!(trunk == candidate, "revert conflict: freeze required");
        checkpoint(root, selected, "before_revert")?;
        git(
            root,
            &["update-ref", "refs/heads/trunk", &revert, &candidate],
        )?;
        checkpoint(root, selected, "after_revert")?;
    }
    once(&c, "compensation", &revert)?;
    checkpoint(root, selected, "recovery_terminal")?;
    // Dependent rejected versions are terminal; a new generation may be explicitly requeued.
    terminal(&c, "parent:1", "rejected")?;
    terminal(&c, "child:1", "rejected_dependency")?;
    checkpoint(root, selected, "dependent_rejection")?;
    once(&c, "requeue", "child:2:parent=accepted")?;
    checkpoint(root, selected, "dependent_requeue")?;
    terminal(&c, "child:2", "cancelled")?;
    Ok(())
}
fn inspect(root: &Path) -> Result<()> {
    let c = db(root)?;
    ensure!(
        c.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))? == "wal",
        "not WAL"
    );
    let id = need(&c, "identity")?;
    let submitted = need(&c, "submitted")?;
    let base = need(&c, "base")?;
    ensure!(need(&c, "ack")? == submitted, "ack mismatch");
    ensure!(
        git(root, &["rev-parse", "refs/changes/same/2"])? == submitted,
        "lost acknowledged object"
    );
    ensure!(
        git(root, &["rev-parse", "refs/drafts/alice"])? == submitted,
        "lost draft"
    );
    git(root, &["cat-file", "-e", &submitted])?;
    let chain = git(
        root,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            "refs/heads/trunk",
        ],
    )?;
    let commits: Vec<_> = chain.lines().collect();
    ensure!(commits.len() == 3, "duplicate/missing publication");
    ensure!(commits[0] == base, "baseline changed");
    let candidate = commits[1];
    let revert = commits[2];
    let msg = git(root, &["show", "-s", "--format=%B", candidate])?;
    ensure!(
        msg.lines().any(|l| l == format!("Submission: {id}")),
        "wrong exact identity"
    );
    ensure!(id == format!("same:2:{submitted}"), "wrong generation");
    ensure!(
        git(root, &["rev-parse", &format!("{candidate}^")])? == base,
        "wrong parent"
    );
    let tree = git(root, &["rev-parse", &format!("{candidate}^{{tree}}")])?;
    let policy = git(root, &["rev-parse", &format!("{base}^{{tree}}")])?;
    ensure!(
        need(&c, "evidence")? == format!("{candidate}|{tree}|{base}|{policy}|attempt=1|PASS"),
        "invented/stale pass"
    );
    ensure!(
        git(root, &["show", &format!("{candidate}:file.txt")])? == "submission",
        "false pass"
    );
    let expected = format!(
        "file-fallback:no-parser\n{}",
        git(
            root,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-status",
                "-r",
                &base,
                candidate
            ]
        )?
    );
    ensure!(need(&c, "manifest")? == expected, "invented manifest index");
    ensure!(
        git(root, &["notes", "--ref=manifests", "show", candidate])? == expected,
        "invented manifest note"
    );
    let original: u64 = fs::read_to_string(root.join("expected-incident"))?.parse()?;
    ensure!(
        need(&c, "incident_start")? == original.to_string(),
        "reset incident start"
    );
    ensure!(
        need(&c, "fix_deadline")? == (original + 100).to_string(),
        "reset deadline"
    );
    ensure!(need(&c, "culprit")? == candidate, "wrong culprit");
    ensure!(
        need(&c, "requeue")? == "child:2:parent=accepted",
        "wrong dependency requeue"
    );
    ensure!(
        need(&c, "full_confirmation")? == "failed:at=99",
        "invented confirmation"
    );
    ensure!(need(&c, "compensation")? == revert, "compensation mismatch");
    ensure!(
        git(root, &["rev-parse", &format!("{revert}^{{tree}}")])? == policy,
        "wrong compensation tree"
    );
    ensure!(
        need(&c, "revert_evidence")? == format!("{revert}|{policy}|{candidate}|PASS"),
        "invented revert pass"
    );
    ensure!(
        git(root, &["show", "-s", "--format=%B", revert])?
            .lines()
            .any(|l| l == format!("Compensates: {id}")),
        "wrong compensation identity"
    );
    for (k, v) in [
        ("grant", "high:expires=20"),
        ("revoke", "high:at=12"),
        ("expiry", "at=20:normal"),
        ("grace_notice", "owner=alice:deadline=30"),
        ("grace_transfer", "at=30:owner=bob"),
        ("attempt_preemption", "attempt=1:cancelled:at=31"),
    ] {
        ensure!(need(&c, k)? == v, "lost priority transition {k}");
    }
    let count: i64 = c.query_row("SELECT count(*) FROM outcomes", [], |r| r.get(0))?;
    ensure!(count == 4, "missing/duplicate terminal outcome");
    for (identity, outcome) in [
        (&*id, "landed"),
        ("parent:1", "rejected"),
        ("child:1", "rejected_dependency"),
        ("child:2", "cancelled"),
    ] {
        let actual: String = c.query_row(
            "SELECT outcome FROM outcomes WHERE identity=?",
            [identity],
            |r| r.get(0),
        )?;
        ensure!(actual == outcome, "incorrect terminal outcome");
    }
    Ok(())
}
fn spawn(root: &Path, point: &str) -> Result<std::process::Child> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("worker.log"))?;
    Ok(Command::new(env::current_exe()?)
        .arg("worker")
        .arg(root)
        .arg(point)
        .env(
            "S7_LOGICAL_TIME",
            if point.is_empty() { "20" } else { "10" },
        )
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?)
}
fn complete(root: &Path) -> Result<()> {
    ensure!(spawn(root, "")?.wait()?.success(), "recovery worker failed");
    Ok(())
}
fn run(root: &Path) -> Result<()> {
    ensure!(!root.exists(), "use a new root");
    fs::create_dir_all(root)?;
    let mut summary = fs::File::create(root.join("summary.log"))?;
    writeln!(
        summary,
        "S7 native process-kill experiment; platform={}; checkpoints={}",
        env::consts::OS,
        POINTS.len()
    )?;
    for point in POINTS {
        let start = Instant::now();
        let case = root.join(point);
        init(&case)?;
        let mut child = spawn(&case, point)?;
        let wait = Instant::now();
        while !case.join("ready").exists() {
            if let Some(status) = child.try_wait()? {
                bail!("worker exited before {point}: {status}");
            }
            if wait.elapsed() > Duration::from_secs(30) {
                child.kill()?;
                child.wait()?;
                bail!("checkpoint timeout {point}");
            }
            thread::sleep(Duration::from_millis(5));
        }
        // A distinct process cannot acquire the active OS lock. Stale lock file remains after kill.
        ensure!(
            !spawn(&case, "")?.wait()?.success(),
            "duplicate engine acquired lock"
        );
        child.kill()?;
        if POINTS.iter().position(|p| p == point).unwrap()
            >= POINTS.iter().position(|p| *p == "fix_window").unwrap()
        {
            fs::write(case.join("expected-incident"), "10")?;
        }
        ensure!(
            !child.wait()?.success(),
            "kill unexpectedly successful exit"
        );
        complete(&case)?;
        let log = fs::File::create(case.join("retry.log"))?;
        ensure!(
            Command::new(env::current_exe()?)
                .arg("worker")
                .arg(&case)
                .env("S7_LOGICAL_TIME", "50")
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .status()?
                .success(),
            "later-clock recovery failed"
        );
        inspect(&case)?;
        writeln!(
            summary,
            "PASS checkpoint={point} duplicate-lock=refused stale-lock=recovered retries=2 elapsed_ms={}",
            start.elapsed().as_millis()
        )?;
        summary.flush()?;
    }
    let repair = root.join("missing-note-index");
    init(&repair)?;
    complete(&repair)?;
    let candidate = need(&db(&repair)?, "candidate")?;
    git(&repair, &["notes", "--ref=manifests", "remove", &candidate])?;
    db(&repair)?.execute("DELETE FROM kv WHERE k='manifest'", [])?;
    complete(&repair)?;
    inspect(&repair)?;
    writeln!(summary, "PASS missing note/index rebuilt from Git diff")?;
    let stale = root.join("stale-pass");
    init(&stale)?;
    put(&db(&stale)?, "evidence", "wrong-parent|PASS")?;
    ensure!(!spawn(&stale, "")?.wait()?.success(), "stale pass accepted");
    ensure!(
        git(&stale, &["rev-parse", "refs/heads/trunk"])? == need(&db(&stale)?, "base")?,
        "stale pass landed"
    );
    writeln!(summary, "PASS stale evidence refuses publication")?;
    // Independent inspector mutation tests prove negative observations are not silently accepted.
    let case = root.join("inspector-negatives");
    init(&case)?;
    complete(&case)?;
    inspect(&case)?;
    for (key, bad) in [
        ("identity", "same:1:wrong"),
        ("evidence", "invented PASS"),
        ("manifest", "invented path"),
        ("fix_deadline", "101"),
        ("compensation", "wrong"),
    ] {
        let c = db(&case)?;
        let original = need(&c, key)?;
        put(&c, key, bad)?;
        ensure!(
            inspect(&case).is_err(),
            "inspector accepted corrupted {key}"
        );
        put(&c, key, &original)?;
        writeln!(summary, "PASS inspector rejects {key}")?;
    }
    git(&case, &["update-ref", "-d", "refs/changes/same/2"])?;
    ensure!(inspect(&case).is_err(), "inspector accepted lost ack ref");
    writeln!(summary, "PASS inspector rejects lost acknowledgment ref")?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    ensure!(
        args.len() >= 3,
        "usage: s7 run|worker|inspect ROOT [checkpoint]"
    );
    let root = PathBuf::from(&args[2]);
    match args[1].as_str() {
        "run" => run(&root),
        "worker" => worker(&root, args.get(3).map(String::as_str).unwrap_or("")),
        "inspect" => inspect(&root),
        _ => bail!("unknown command"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadline_is_insert_once() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE kv(k TEXT PRIMARY KEY,v TEXT NOT NULL)")?;
        once(&c, "deadline", "100")?;
        once(&c, "deadline", "200")?;
        assert_eq!(need(&c, "deadline")?, "100");
        Ok(())
    }
    #[test]
    fn terminal_cannot_be_rewritten() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE outcomes(identity TEXT PRIMARY KEY,outcome TEXT NOT NULL)")?;
        terminal(&c, "v1", "rejected")?;
        terminal(&c, "v1", "rejected")?;
        assert!(terminal(&c, "v1", "landed").is_err());
        terminal(&c, "v2", "landed")?;
        Ok(())
    }
    #[test]
    fn missing_evidence_is_error() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch("CREATE TABLE kv(k TEXT PRIMARY KEY,v TEXT NOT NULL)")?;
        assert!(need(&c, "evidence").is_err());
        Ok(())
    }
}
