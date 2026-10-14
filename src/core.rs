//! Revision semantics and local coordination; no transport or presentation.
use crate::{
    protocol::Submission,
    storage_git::{Git, command, execute, text},
};
use anyhow::{Result, bail, ensure};
use fs2::FileExt;
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct Repository {
    pub root: PathBuf,
    pub control: PathBuf,
    pub git: Git,
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub fn overlap(a: &str, b: &str) -> bool {
    a == b
        || a.strip_prefix(b).is_some_and(|s| s.starts_with('/'))
        || b.strip_prefix(a).is_some_and(|s| s.starts_with('/'))
}
pub fn validate_path(path: &str) -> Result<String> {
    let p = path.strip_suffix('/').unwrap_or(path);
    ensure!(
        !p.is_empty()
            && !p.starts_with('/')
            && !p.contains(['\\', '\0'])
            && !p.split('/').any(|c| matches!(c, "" | "." | "..")),
        "Invalid repository-relative path: {path:?}"
    );
    Ok(p.into())
}
fn agent_id(agent: &str) -> Result<()> {
    ensure!(
        !agent.is_empty()
            && agent.len() <= 64
            && agent.as_bytes()[0].is_ascii_alphanumeric()
            && agent
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Agent ID must be 1–64 ASCII letters/digits, '_' or '-', starting with a letter/digit"
    );
    Ok(())
}
fn paths(bytes: &[u8]) -> Result<Vec<String>> {
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| Ok(String::from_utf8(p.to_vec())?))
        .collect()
}
fn string_path(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 filesystem path is unsupported"))
}
impl Repository {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = crate::storage_git::canonical_git_path(root)?;
        let control = root.join(".mai");
        ensure!(
            control.join("metadata.sqlite3").is_file() && control.join("repo.git").is_dir(),
            "No mai repository at {} (run 'mai init' first)",
            root.display()
        );
        Ok(Self {
            git: Git {
                bare: control.join("repo.git"),
            },
            root,
            control,
        })
    }
    pub fn init(root: impl AsRef<Path>, source: Option<&str>) -> Result<Self> {
        let root = root.as_ref();
        ensure!(
            !root.exists() || fs::read_dir(root)?.next().is_none(),
            "Target must be an empty directory. Use --from to import an existing Git repository"
        );
        if let Some(s) = source {
            ensure!(
                !s.is_empty() && !s.starts_with('-'),
                "Invalid import source"
            );
        }
        fs::create_dir_all(root)?;
        let root = crate::storage_git::canonical_git_path(root)?;
        let control = root.join(".mai");
        fs::create_dir(&control)?;
        let git = Git {
            bare: control.join("repo.git"),
        };
        let mut c = command();
        c.args(["init", "--bare", "--initial-branch=main", "--quiet"])
            .arg(&git.bare);
        execute(c, &[])?;
        let head = if let Some(source) = source {
            git.run(&["fetch", "--quiet", "--no-tags", source, "HEAD"])?;
            git.text(&["rev-parse", "FETCH_HEAD"])?
        } else {
            let tree = text(git.input(&["mktree"], &[], &[])?)?;
            text(git.input(
                &["commit-tree", &tree, "-m", "Initialize mai"],
                &[],
                &identity("mai", "system@mai.invalid"),
            )?)?
        };
        git.run(&["update-ref", "refs/heads/main", &head])?;
        git.run(&["update-ref", "refs/mai/base", &head])?;
        fs::create_dir(root.join("workspaces"))?;
        let conn = Connection::open(control.join("metadata.sqlite3"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE leases(path TEXT PRIMARY KEY,agent TEXT NOT NULL,expires_at REAL NOT NULL); CREATE TABLE workspaces(agent TEXT PRIMARY KEY,created_at REAL NOT NULL);")?;
        Ok(Self { root, control, git })
    }
    fn db(&self) -> Result<Connection> {
        let db = Connection::open(self.control.join("metadata.sqlite3"))?;
        db.busy_timeout(Duration::from_secs(20))?;
        db.pragma_update(None, "synchronous", "FULL")?;
        Ok(db)
    }
    fn transaction(db: &mut Connection) -> Result<Transaction<'_>> {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM leases WHERE expires_at <= ?", [now()])?;
        Ok(tx)
    }
    pub fn workspace_path(&self, agent: &str) -> Result<PathBuf> {
        agent_id(agent)?;
        Ok(self.root.join("workspaces").join(agent))
    }
    fn workspace(&self, agent: &str) -> Result<PathBuf> {
        let p = self.workspace_path(agent)?;
        ensure!(
            !fs::symlink_metadata(self.root.join("workspaces"))?
                .file_type()
                .is_symlink(),
            "Workspace registry is a symlink"
        );
        ensure!(
            p.is_dir() && !fs::symlink_metadata(&p)?.file_type().is_symlink(),
            "No workspace for '{agent}', or workspace is a symlink"
        );
        ensure!(
            fs::canonicalize(&p)?.starts_with(fs::canonicalize(self.root.join("workspaces"))?),
            "Workspace escapes registry"
        );
        let common = self.git.wc_text(
            &p,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        ensure!(
            fs::canonicalize(common)? == fs::canonicalize(&self.git.bare)?,
            "Workspace does not belong to this authority"
        );
        Ok(p)
    }
    fn agent_lock(&self, agent: &str) -> Result<File> {
        agent_id(agent)?;
        let locks = self.control.join("locks");
        fs::create_dir_all(&locks)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(locks.join(format!("{agent}.lock")))?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }
    pub fn head(&self) -> Result<String> {
        self.git.text(&["rev-parse", "refs/heads/main"])
    }
    pub fn revision_at(&self, head: &str) -> Result<u64> {
        Ok(self
            .git
            .text(&[
                "rev-list",
                "--first-parent",
                "--count",
                &format!("refs/mai/base..{head}"),
            ])?
            .parse()?)
    }
    pub fn revision(&self) -> Result<u64> {
        self.revision_at(&self.head()?)
    }
    pub fn create_workspace(&self, agent: &str) -> Result<Value> {
        let _guard = self.agent_lock(agent)?;
        let path = self.workspace_path(agent)?;
        ensure!(
            !fs::symlink_metadata(self.root.join("workspaces"))?
                .file_type()
                .is_symlink(),
            "Workspace registry is a symlink"
        );
        let mut db = self.db()?;
        let tx = Self::transaction(&mut db)?;
        ensure!(
            !path.exists()
                && tx.query_row(
                    "SELECT count(*) FROM workspaces WHERE agent=?",
                    [agent],
                    |r| r.get::<_, i64>(0)
                )? == 0,
            "Workspace '{agent}' already exists"
        );
        let head = self.head()?;
        self.git.run(&[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            string_path(&path)?,
            &head,
        ])?;
        tx.execute(
            "INSERT INTO workspaces(agent,created_at) VALUES(?,?)",
            params![agent, now()],
        )?;
        tx.commit()?;
        Ok(
            json!({"agent":agent,"path":path,"base_commit":head,"revision":self.revision_at(&head)?}),
        )
    }
    pub fn refresh(&self, agent: &str, discard: bool) -> Result<Value> {
        let _guard = self.agent_lock(agent)?;
        let path = self.workspace(agent)?;
        let dirty = !self
            .git
            .workspace(&path, &["status", "--porcelain", "-z"])?
            .is_empty();
        ensure!(
            !dirty || discard,
            "Workspace has unsubmitted changes; use 'diff', submit, or refresh --discard (destructive)"
        );
        let head = self.head()?;
        self.git.workspace(&path, &["reset", "--hard", &head])?;
        if discard {
            self.git.workspace(&path, &["clean", "-fd"])?;
        }
        Ok(
            json!({"agent":agent,"revision":self.revision_at(&head)?,"commit":head,"discarded":dirty && discard}),
        )
    }
    pub fn lease(&self, agent: &str, raw: &[String], ttl: u64) -> Result<Value> {
        self.workspace(agent)?;
        ensure!(
            (1..=86400).contains(&ttl),
            "Lease TTL must be between 1 and 86400 seconds"
        );
        let mut paths = Vec::new();
        for p in raw {
            let p = validate_path(p)?;
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        ensure!(
            !paths.is_empty(),
            "Provide at least one repository-relative path"
        );
        let mut db = self.db()?;
        let tx = Self::transaction(&mut db)?;
        Self::check_leases(&tx, agent, &paths)?;
        let expires = now() + ttl as f64;
        for p in &paths {
            tx.execute("INSERT INTO leases(path,agent,expires_at) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET agent=excluded.agent,expires_at=excluded.expires_at",params![p,agent,expires])?;
        }
        tx.commit()?;
        Ok(json!({"agent":agent,"paths":paths,"expires_at":expires}))
    }
    fn check_leases(tx: &Transaction, agent: &str, paths: &[String]) -> Result<()> {
        let mut stmt =
            tx.prepare("SELECT path,agent FROM leases WHERE expires_at > ? ORDER BY path")?;
        let rows = stmt.query_map([now()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (p, owner) = row?;
            for changed in paths {
                ensure!(
                    owner == agent || !overlap(changed, &p),
                    "Cannot submit/reserve '{changed}': reserved by '{owner}' at '{p}'"
                );
            }
        }
        Ok(())
    }
    pub fn release(&self, agent: &str, paths: &[String]) -> Result<Value> {
        self.workspace(agent)?;
        let paths = paths
            .iter()
            .map(|p| validate_path(p))
            .collect::<Result<Vec<_>>>()?;
        let mut db = self.db()?;
        let tx = Self::transaction(&mut db)?;
        let count = if paths.is_empty() {
            tx.execute("DELETE FROM leases WHERE agent=?", [agent])?
        } else {
            let mut n = 0;
            for p in paths {
                n += tx.execute(
                    "DELETE FROM leases WHERE agent=? AND path=?",
                    params![agent, p],
                )?;
            }
            n
        };
        tx.commit()?;
        Ok(json!({"agent":agent,"released":count}))
    }
    pub fn leases(&self) -> Result<Value> {
        let db = self.db()?;
        let mut stmt = db.prepare(
            "SELECT path,agent,expires_at FROM leases WHERE expires_at > ? ORDER BY path",
        )?;
        let rows=stmt.query_map([now()],|r| Ok(json!({"path":r.get::<_,String>(0)?,"agent":r.get::<_,String>(1)?,"expires_at":r.get::<_,f64>(2)?})))?;
        Ok(Value::Array(rows.collect::<rusqlite::Result<Vec<_>>>()?))
    }
    pub fn status(&self) -> Result<Value> {
        let head = self.head()?;
        let db = self.db()?;
        let mut stmt = db.prepare("SELECT agent FROM workspaces ORDER BY agent")?;
        let agents = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut workspaces = Vec::new();
        for agent in agents {
            let path = self.workspace_path(&agent)?;
            let state = self.workspace(&agent).and_then(|p| {
                Ok((
                    self.git.wc_text(&p, &["rev-parse", "HEAD"])?,
                    !self
                        .git
                        .workspace(&p, &["status", "--porcelain", "-z"])?
                        .is_empty(),
                ))
            });
            let (base, dirty) = match state {
                Ok((b, d)) => (Some(b), Some(d)),
                Err(_) => (None, None),
            };
            workspaces.push(json!({"agent":agent,"path":path,"base_commit":base,"dirty":dirty}));
        }
        Ok(
            json!({"revision":self.revision_at(&head)?,"head":head,"workspaces":workspaces,"leases":self.leases()?}),
        )
    }
    pub fn diff(&self, agent: &str) -> Result<String> {
        let _guard = self.agent_lock(agent)?;
        let p = self.workspace(agent)?;
        self.git.workspace(&p, &["add", "--all"])?;
        Ok(String::from_utf8(self.git.workspace(
            &p,
            &["diff", "--cached", "--no-ext-diff", "--no-textconv", "HEAD"],
        )?)?)
    }
    pub fn show(&self, revision: &str, path: &str) -> Result<Vec<u8>> {
        let p = validate_path(path)?;
        let head = self.head()?;
        let sha = if revision == "HEAD" {
            head
        } else {
            let n = revision
                .strip_prefix('r')
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .ok_or_else(|| anyhow::anyhow!("Revision must be r0, r1, ... or HEAD"))?
                .parse::<u64>()?;
            let current = self.revision_at(&head)?;
            ensure!(
                n <= current,
                "Revision {revision} not found; trunk is at r{current}"
            );
            self.git
                .text(&["rev-parse", &format!("{head}~{}", current - n)])?
        };
        self.git.run(&["show", &format!("{sha}:{p}")])
    }
    pub fn history(&self, limit: u64) -> Result<Value> {
        ensure!(limit > 0, "Limit must be at least 1");
        let head = self.head()?;
        let revision = self.revision_at(&head)?;
        let shas = self.git.text(&[
            "rev-list",
            "--first-parent",
            "--max-count",
            &limit.min(revision + 1).to_string(),
            &head,
        ])?;
        let mut items = Vec::new();
        for (i, sha) in shas.lines().enumerate() {
            let info = self
                .git
                .text(&["show", "-s", "--format=%an%n%s%n%b", sha])?;
            let mut lines = info.lines();
            let author = lines.next().unwrap_or("");
            let message = lines.next().unwrap_or("");
            let agent = lines
                .filter_map(|l| l.strip_prefix("Agent-ID: "))
                .next_back()
                .unwrap_or("import/system");
            items.push(json!({"revision":revision-i as u64,"commit":sha,"author":author,"message":message,"agent":agent}));
        }
        Ok(Value::Array(items))
    }
    pub fn link_github(&self, url: &str) -> Result<Value> {
        ensure!(
            !url.is_empty() && !url.starts_with('-'),
            "Provide a Git remote URL or path"
        );
        if self.git.run(&["remote", "get-url", "github"]).is_ok() {
            self.git.run(&["remote", "set-url", "github", url])?;
        } else {
            self.git.run(&["remote", "add", "github", url])?;
        }
        Ok(json!({"remote":"github","url":url}))
    }
    pub fn sync_github(&self) -> Result<Value> {
        let head = self.head()?;
        self.git
            .run(&["push", "github", &format!("{head}:refs/heads/main")])?;
        Ok(json!({"remote":"github","commit":head,"revision":self.revision_at(&head)?}))
    }
    pub fn submit(&self, agent: &str, message: &str, check: Option<&str>) -> Result<Submission> {
        let _guard = self.agent_lock(agent)?;
        let path = self.workspace(agent)?;
        ensure!(
            !message.trim().is_empty(),
            "A non-empty commit message is required"
        );
        let base = self.git.wc_text(&path, &["rev-parse", "HEAD"])?;
        let receipt_ref = format!("refs/mai/receipts/{agent}");
        let old_receipt = self
            .git
            .text(&["rev-parse", "--verify", &receipt_ref])
            .unwrap_or_else(|_| "0".repeat(40));
        // Receipts accelerate recovery; only the authoritative main chain proves
        // acceptance. Loose Git refs are not a filesystem-wide crash transaction.
        let current_head = self.head()?;
        let mut recovery = None;
        if old_receipt != "0".repeat(40)
            && self
                .git
                .run(&["merge-base", "--is-ancestor", &old_receipt, &current_head])
                .is_ok()
        {
            let body = self
                .git
                .text(&["show", "-s", "--format=%b", &old_receipt])?;
            if trailers_match(&body, agent, &base) {
                recovery = Some(old_receipt.clone());
            }
        }
        if recovery.is_none() && base != current_head {
            // Covers termination between individual loose-ref replacements.
            let log = self.git.text(&[
                "log",
                "--first-parent",
                "--format=%H%x00%B%x00",
                &format!("{base}..{current_head}"),
            ])?;
            let records = log.split('\0').collect::<Vec<_>>();
            for pair in records.chunks_exact(2) {
                if trailers_match(pair[1], agent, &base) {
                    recovery = Some(pair[0].trim().to_owned());
                    break;
                }
            }
        }
        if let Some(commit) = recovery {
            let parent = self.git.text(&["rev-parse", &format!("{commit}^")])?;
            let files = paths(&self.git.run(&[
                "diff",
                "--no-renames",
                "--name-only",
                "-z",
                &parent,
                &commit,
            ])?)?;
            return Ok(Submission {
                revision: self.revision_at(&commit)?,
                commit,
                agent: agent.into(),
                files,
                rebased: base != parent,
                recovered: true,
                workspace_updated: false,
                retries: 0,
                lock_wait_ms: 0.,
                lock_hold_ms: 0.,
            });
        }
        if let Some(check) = check {
            let output = verification_command(check).current_dir(&path).output()?;
            if !output.status.success() {
                let mut bytes = output.stdout;
                bytes.extend(output.stderr);
                let start = bytes.len().saturating_sub(1000);
                bail!(
                    "Verification command failed (exit {}): {}",
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&bytes[start..]).trim()
                );
            }
        }
        // Parallel preparation: workspace staging/hashing and immutable index snapshot.
        self.git.workspace(&path, &["add", "--all"])?;
        let changed = paths(&self.git.workspace(
            &path,
            &[
                "diff",
                "--cached",
                "--no-renames",
                "--name-only",
                "-z",
                "HEAD",
            ],
        )?)?;
        ensure!(!changed.is_empty(), "There are no changes to submit");
        let mut entries = BTreeMap::new();
        let raw = self.git.workspace(&path, &["ls-files", "--stage", "-z"])?;
        for record in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let split = record
                .iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| anyhow::anyhow!("Invalid index entry"))?;
            let meta = std::str::from_utf8(&record[..split])?
                .split_whitespace()
                .collect::<Vec<_>>();
            ensure!(
                meta.len() == 3 && meta[2] == "0",
                "Unmerged index entries found; resolve conflicts before submission"
            );
            let name = String::from_utf8(record[split + 1..].to_vec())?;
            entries.insert(name, (meta[0].to_owned(), meta[1].to_owned()));
        }
        let mut data = Vec::new();
        for p in changed.iter().filter(|p| !entries.contains_key(*p)) {
            data.extend_from_slice(format!("0 {}\t{p}\0", "0".repeat(40)).as_bytes());
        }
        for p in changed.iter().filter(|p| entries.contains_key(*p)) {
            let (mode, oid) = &entries[p];
            data.extend_from_slice(format!("{mode} {oid}\t{p}\0").as_bytes());
        }
        let mut wait_ms = 0.;
        let mut hold_ms = 0.;
        for retries in 0..64 {
            let current = self.head()?;
            ensure!(
                self.git
                    .run(&["merge-base", "--is-ancestor", &base, &current])
                    .is_ok(),
                "Workspace base is not an ancestor of trunk; recreate the workspace"
            );
            let upstream = paths(&self.git.run(&[
                "diff",
                "--no-renames",
                "--name-only",
                "-z",
                &base,
                &current,
            ])?)?;
            let collisions = changed
                .iter()
                .filter(|p| upstream.iter().any(|q| overlap(p, q)))
                .cloned()
                .collect::<BTreeSet<_>>();
            ensure!(
                collisions.is_empty(),
                "Stale workspace: overlapping changes on trunk: {}",
                collisions.into_iter().collect::<Vec<_>>().join(", ")
            );
            let directory = tempfile::tempdir_in(&self.control)?;
            let index = directory.path().join("index");
            let index = string_path(&index)?;
            let env = [("GIT_INDEX_FILE", index)];
            self.git.input(&["read-tree", &current], &[], &env)?;
            self.git
                .input(&["update-index", "-z", "--index-info"], &data, &env)?;
            let tree = text(self.git.input(&["write-tree"], &[], &env)?)?;
            let name = format!("agent/{agent}");
            let email = format!("{agent}@mai.invalid");
            let desc = format!(
                "{}\n\nAgent-ID: {agent}\nWorkspace-Base: {base}\n",
                message.trim()
            );
            let commit = text(self.git.input(
                &["commit-tree", &tree, "-p", &current, "-m", &desc],
                &[],
                &identity(&name, &email),
            )?)?;
            let new_revision = self.revision_at(&current)? + 1;
            // Serialized publication: recheck head and live leases, atomic Git CAS.
            let mut db = self.db()?;
            let waiting = Instant::now();
            let tx = Self::transaction(&mut db)?;
            wait_ms += waiting.elapsed().as_secs_f64() * 1000.;
            let held = Instant::now();
            if self.head()? != current {
                drop(tx);
                hold_ms += held.elapsed().as_secs_f64() * 1000.;
                std::thread::sleep(Duration::from_millis(1 + (retries as u64).min(10)));
                continue;
            }
            Self::check_leases(&tx, agent, &changed)?;
            self.git
                .publish(&current, &commit, &receipt_ref, &old_receipt)?;
            // Lease pruning is not acceptance metadata. Git is authoritative even
            // if SQLite commit fails after Git publication.
            let _ = tx.commit();
            hold_ms += held.elapsed().as_secs_f64() * 1000.;
            let workspace_updated = self
                .git
                .workspace(&path, &["reset", "--hard", &commit])
                .is_ok();
            return Ok(Submission {
                revision: new_revision,
                commit,
                agent: agent.into(),
                files: changed,
                rebased: base != current,
                recovered: false,
                workspace_updated,
                retries,
                lock_wait_ms: wait_ms,
                lock_hold_ms: hold_ms,
            });
        }
        bail!("Publication contention exceeded 64 attempts; no changes accepted, retry later")
    }
}
fn identity<'a>(name: &'a str, email: &'a str) -> [(&'a str, &'a str); 4] {
    [
        ("GIT_AUTHOR_NAME", name),
        ("GIT_AUTHOR_EMAIL", email),
        ("GIT_COMMITTER_NAME", "mai"),
        ("GIT_COMMITTER_EMAIL", "system@mai.invalid"),
    ]
}

fn trailers_match(body: &str, agent: &str, base: &str) -> bool {
    body.lines()
        .rev()
        .find_map(|l| l.strip_prefix("Agent-ID: "))
        == Some(agent)
        && body
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix("Workspace-Base: "))
            == Some(base)
}

// A check is explicitly trusted shell input, never a remote server script.
fn verification_command(check: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new("cmd.exe");
        // cmd has its own quoting grammar; raw_arg preserves the caller's shell
        // syntax. /S removes our outer quotes, including for quoted executable paths.
        command
            .args(["/D", "/S", "/C"])
            .raw_arg(format!("\"{check}\""));
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", check]);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::paths;

    #[test]
    fn git_path_lists_reject_invalid_utf8_without_a_filesystem() {
        assert_eq!(
            paths(b"src/valid\0binary\0").unwrap(),
            ["src/valid", "binary"]
        );
        assert!(paths(b"valid\0\xff\0").is_err());
    }
}
