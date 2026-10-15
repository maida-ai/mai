//! Disposable local forge topology, not the mai engine or a GitHub API mock.
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Instant,
};

type Files = BTreeMap<String, String>;
type Result<T> = std::result::Result<T, String>;

fn ensure(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn exact(actual: &Files, expected: &Files) -> Result<()> {
    ensure(
        actual == expected,
        &format!("tree mismatch: actual={actual:?} expected={expected:?}"),
    )
}

fn revision(message: &str) -> Result<u32> {
    let values: Vec<_> = message
        .lines()
        .filter_map(|line| line.strip_prefix("Mai-Revision: r"))
        .collect();
    ensure(values.len() == 1, "missing or duplicate revision trailer")?;
    values[0]
        .parse()
        .map_err(|_| "invalid revision number".into())
}

fn gate(files: &Files, expected: &Files) -> Result<()> {
    ensure(
        !files.values().any(|value| value.contains("INVALID")),
        "fixture gate rejected INVALID content",
    )?;
    exact(files, expected)
}

fn conflict_owner(owner: &str) -> Result<&str> {
    ensure(
        !owner.trim().is_empty() && !owner.contains(['\n', '\r']),
        "configured conflict owner is required",
    )?;
    Ok(owner)
}

fn windows_git_path(path: &str) -> String {
    path.strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('\\', "/")
}

fn git_path(path: &Path) -> Result<String> {
    let value = path.to_str().ok_or("non-UTF8 fixture path")?;
    Ok(if cfg!(windows) {
        windows_git_path(value)
    } else {
        value.into()
    })
}

struct Harness {
    root: PathBuf,
    log: File,
    assertions: usize,
    owner: String,
}

impl Harness {
    fn check(&mut self, condition: bool, message: &str) -> Result<()> {
        ensure(condition, message)?;
        self.assertions += 1;
        writeln!(self.log, "PASS {message}").map_err(|e| e.to_string())?;
        Ok(())
    }

    fn raw(&mut self, repo: &Path, args: &[&str], input: Option<&str>) -> Result<Output> {
        let start = Instant::now();
        writeln!(self.log, "git cwd={} args={args:?}", repo.display())
            .map_err(|e| e.to_string())?;
        self.log.flush().map_err(|e| e.to_string())?;
        let mut command = Command::new("git");
        // All inputs are synthetic. Avoid user hooks, signing, global attributes and config.
        command
            .current_dir(repo)
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "protocol.file.allow=always",
            ])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                git_path(&self.root.join("empty-config"))?,
            )
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env("GIT_AUTHOR_NAME", "S5 Fixture")
            .env("GIT_AUTHOR_EMAIL", "s5@example.invalid")
            .env("GIT_COMMITTER_NAME", "S5 Fixture")
            .env("GIT_COMMITTER_EMAIL", "s5@example.invalid")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        if let Some(input) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        writeln!(
            self.log,
            "exit={:?} elapsed_us={}\nstdout={}\nstderr={}",
            output.status.code(),
            start.elapsed().as_micros(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .map_err(|e| e.to_string())?;
        self.log.flush().map_err(|e| e.to_string())?;
        Ok(output)
    }

    fn git(&mut self, repo: &Path, args: &[&str]) -> Result<String> {
        let output = self.raw(repo, args, None)?;
        ensure(
            output.status.success(),
            &format!(
                "git failed {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn commit_tree(
        &mut self,
        repo: &Path,
        tree: &str,
        parents: &[&str],
        message: &str,
    ) -> Result<String> {
        let mut args = vec!["commit-tree", tree];
        for parent in parents {
            args.extend(["-p", parent]);
        }
        let output = self.raw(repo, &args, Some(message))?;
        ensure(output.status.success(), "commit-tree failed")?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().into())
    }

    fn files(&mut self, repo: &Path, tree: &str) -> Result<Files> {
        let mut files = Files::new();
        for name in self
            .git(repo, &["ls-tree", "-r", "--name-only", tree])?
            .lines()
        {
            let output = self.raw(repo, &["show", &format!("{tree}:{name}")], None)?;
            ensure(output.status.success(), "read tree blob failed")?;
            files.insert(
                name.into(),
                String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
            );
        }
        Ok(files)
    }

    fn edit(&mut self, repo: &Path, name: &str, contents: &str, message: &str) -> Result<String> {
        fs::write(repo.join(name), contents).map_err(|e| e.to_string())?;
        self.git(repo, &["add", "--", name])?;
        self.git(repo, &["commit", "-m", message])?;
        self.git(repo, &["rev-parse", "HEAD"])
    }

    fn clone_repo(&mut self, parent: &Path, source: &Path, name: &str) -> Result<PathBuf> {
        self.git(parent, &["clone", "--no-local", &git_path(source)?, name])?;
        Ok(parent.join(name))
    }

    fn merge(&mut self, repo: &Path, left: &str, right: &str) -> Result<String> {
        let output = self.raw(repo, &["merge-tree", "--write-tree", left, right], None)?;
        ensure(
            output.status.success(),
            &format!(
                "unexpected conflict: {}",
                String::from_utf8_lossy(&output.stdout)
            ),
        )?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .ok_or("missing merge tree")?
            .into())
    }

    fn land(
        &mut self,
        repo: &Path,
        tree: &str,
        parents: &[&str],
        number: u32,
        kind: &str,
        expected: &Files,
    ) -> Result<String> {
        let actual = self.files(repo, tree)?;
        gate(&actual, expected)?;
        writeln!(self.log, "GATE pass tree={tree} parents={parents:?} revision=r{number} kind={kind} policy=compiled-exact-fixture-v1").map_err(|e| e.to_string())?;
        let commit = self.commit_tree(
            repo,
            tree,
            parents,
            &format!("{kind}\n\nMai-Revision: r{number}\n"),
        )?;
        self.git(
            repo,
            &["update-ref", "refs/heads/main", &commit, parents[0]],
        )?;
        let landed_tree = self.git(repo, &["rev-parse", &format!("{commit}^{{tree}}")])?;
        self.check(landed_tree == tree, "landed tree equals checked tree")?;
        let actual_parents = self.git(repo, &["show", "-s", "--format=%P", &commit])?;
        self.check(
            actual_parents == parents.join(" "),
            "exact first/second parent provenance",
        )?;
        let message = self.git(repo, &["show", "-s", "--format=%B", &commit])?;
        self.check(
            revision(&message)? == number,
            "revision trailer matches assigned rN",
        )?;
        Ok(commit)
    }

    fn mode(&mut self, mode: &str) -> Result<()> {
        let dir = self.root.join(mode);
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        self.git(
            &dir,
            &["init", "--bare", "--initial-branch=main", "forge.git"],
        )?;
        let forge = dir.join("forge.git");
        let human = self.clone_repo(&dir, &forge, "human")?;
        let mut expected = Files::from([("shared.txt".into(), "baseline\n".into())]);
        let upstream0 = self.edit(&human, "shared.txt", "baseline\n", "Human baseline")?;
        self.git(&human, &["push", "origin", "HEAD:refs/heads/main"])?;
        let authority = self.clone_repo(&dir, &forge, "authority")?;
        let tree0 = self.git(&authority, &["rev-parse", "HEAD^{tree}"])?;
        let import = self.commit_tree(
            &authority,
            &tree0,
            &[&upstream0],
            "Import baseline\n\nMai-Revision: r0\n",
        )?;
        self.git(
            &authority,
            &["update-ref", "refs/heads/main", &import, &upstream0],
        )?;
        self.git(&authority, &["update-ref", "refs/mai/upstream", &upstream0])?;
        let agent = self.clone_repo(&dir, &authority, "agent")?;
        let merger = self.clone_repo(&dir, &forge, "forge-merger")?;
        let mut trunk = import.clone();
        let mut upstream = upstream0;
        let mut rev = 0;

        for cycle in 1..=3 {
            writeln!(
                self.log,
                "CYCLE mode={mode} cycle={cycle} trunk={trunk} upstream={upstream}"
            )
            .map_err(|e| e.to_string())?;
            self.git(&agent, &["fetch", "origin"])?;
            self.git(&agent, &["checkout", "-B", "work", "origin/main"])?;
            let file = format!("agent-{cycle}.txt");
            let contents = format!("{mode} agent cycle {cycle}\n");
            let submitted = self.edit(&agent, &file, &contents, "Agent change")?;
            self.git(
                &agent,
                &[
                    "push",
                    "origin",
                    &format!("HEAD:refs/mai/submitted/{cycle}"),
                ],
            )?;
            expected.insert(file.clone(), contents);
            let tree = self.merge(&authority, &trunk, &submitted)?;
            rev += 1;
            trunk = self.land(
                &authority,
                &tree,
                &[&trunk],
                rev,
                "Agent landing",
                &expected,
            )?;

            // Export one complete delta against the last successfully synced upstream.
            let export = self.commit_tree(
                &authority,
                &tree,
                &[&upstream],
                &format!("Export {mode} cycle {cycle}\n"),
            )?;
            let exported_diff =
                self.git(&authority, &["diff", "--name-only", &upstream, &export])?;
            self.check(
                exported_diff == file,
                "export contains only new agent delta",
            )?;
            self.git(
                &authority,
                &[
                    "push",
                    "origin",
                    &format!("{export}:refs/heads/export-{mode}-{cycle}"),
                ],
            )?;
            let human_file = format!("human-{cycle}.txt");
            let human_contents = format!("human concurrent cycle {cycle}\n");
            self.git(&human, &["pull", "--ff-only", "origin", "main"])?;
            let concurrent_human = self.edit(
                &human,
                &human_file,
                &human_contents,
                "Concurrent human edit",
            )?;
            self.git(&human, &["push", "origin", "HEAD:refs/heads/main"])?;
            expected.insert(human_file.clone(), human_contents);
            self.git(&merger, &["fetch", "origin"])?;
            let export_ref = format!("origin/export-{mode}-{cycle}");
            if mode == "squash" {
                self.git(&merger, &["checkout", "-B", "merge", "origin/main"])?;
                self.git(&merger, &["merge", "--squash", &export_ref])?;
                self.git(&merger, &["commit", "-m", "Forge squash merge"])?;
            } else {
                self.git(&merger, &["checkout", "-B", "merge", &export_ref])?;
                self.git(&merger, &["rebase", "origin/main"])?;
            }
            self.git(&merger, &["push", "origin", "HEAD:refs/heads/main"])?;
            let merged = self.git(&merger, &["rev-parse", "HEAD"])?;
            let parent = self.git(&merger, &["show", "-s", "--format=%P", "HEAD"])?;
            self.check(
                parent == concurrent_human,
                "forge merge is single-parent after concurrent human commit",
            )?;
            self.check(merged != export, "forge rewrote exported commit identity")?;
            exact(&self.files(&merger, "HEAD")?, &expected)?;
            self.check(
                true,
                "forge tree contains exact agent and concurrent human contents",
            )?;
            self.git(&authority, &["fetch", "origin"])?;
            let candidate = self.merge(&authority, &trunk, &merged)?;
            let before = trunk.clone();
            rev += 1;
            trunk = self.land(
                &authority,
                &candidate,
                &[&trunk, &merged],
                rev,
                "Upstream sync",
                &expected,
            )?;
            let sync_diff = self.git(&authority, &["diff", "--name-only", &before, &trunk])?;
            self.check(
                sync_diff == human_file,
                "sync adds only concurrent human delta without replay",
            )?;
            self.git(
                &authority,
                &["update-ref", "refs/mai/upstream", &merged, &upstream],
            )?;
            upstream = merged;
            self.check(true, "cycle completed with zero spurious conflicts")?;
        }

        // A clean merge whose exact candidate fails validation must not move either ref.
        self.git(&human, &["pull", "--ff-only", "origin", "main"])?;
        self.edit(
            &human,
            "invalid.txt",
            "INVALID upstream fixture\n",
            "Deliberately invalid upstream",
        )?;
        self.git(&human, &["push", "origin", "HEAD:refs/heads/main"])?;
        self.git(&authority, &["fetch", "origin"])?;
        let bad_upstream = self.git(&authority, &["rev-parse", "origin/main"])?;
        let invalid_tree = self.merge(&authority, &trunk, &bad_upstream)?;
        let invalid_files = self.files(&authority, &invalid_tree)?;
        let rejected = gate(&invalid_files, &invalid_files).unwrap_err();
        writeln!(
            self.log,
            "GATE reject tree={invalid_tree} parents=[{trunk},{bad_upstream}] reason={rejected}"
        )
        .map_err(|e| e.to_string())?;
        let current = self.git(&authority, &["rev-parse", "main"])?;
        let recorded = self.git(&authority, &["rev-parse", "refs/mai/upstream"])?;
        self.check(
            current == trunk && recorded == upstream,
            "failed validation preserves trunk and last synced upstream",
        )?;
        // Continue a non-conflicting local change while the rejected upstream remains pending.
        self.git(&agent, &["fetch", "origin"])?;
        self.git(&agent, &["checkout", "-B", "work", "origin/main"])?;
        let safe = self.edit(
            &agent,
            "during-failed-gate.txt",
            "independent of invalid upstream\n",
            "Work during rejected sync",
        )?;
        self.git(
            &agent,
            &[
                "push",
                "origin",
                "HEAD:refs/mai/submitted/during-failed-gate",
            ],
        )?;
        expected.insert(
            "during-failed-gate.txt".into(),
            "independent of invalid upstream\n".into(),
        );
        let safe_tree = self.merge(&authority, &trunk, &safe)?;
        rev += 1;
        trunk = self.land(
            &authority,
            &safe_tree,
            &[&trunk],
            rev,
            "Work during rejected sync",
            &expected,
        )?;
        let actual_upstream = self.git(&forge, &["rev-parse", "main"])?;
        let recorded = self.git(&authority, &["rev-parse", "refs/mai/upstream"])?;
        self.check(
            actual_upstream == bad_upstream && recorded == upstream,
            "continued work during rejected sync preserves forge main and old sync base",
        )?;
        // Human corrects their own upstream with a new commit; harness never rewrites it.
        self.git(&human, &["rm", "invalid.txt"])?;
        self.git(&human, &["commit", "-m", "Human correction"])?;
        self.git(&human, &["push", "origin", "HEAD:refs/heads/main"])?;

        // Both sides now change the same baseline line: a genuine conflict.
        self.git(&agent, &["fetch", "origin"])?;
        self.git(&agent, &["checkout", "-B", "work", "origin/main"])?;
        let local = self.edit(
            &agent,
            "shared.txt",
            "agent owns this line\n",
            "Agent conflicting edit",
        )?;
        self.git(
            &agent,
            &["push", "origin", "HEAD:refs/mai/submitted/conflict"],
        )?;
        expected.insert("shared.txt".into(), "agent owns this line\n".into());
        let tree = self.merge(&authority, &trunk, &local)?;
        rev += 1;
        trunk = self.land(
            &authority,
            &tree,
            &[&trunk],
            rev,
            "Agent conflicting landing",
            &expected,
        )?;
        let human_conflict = self.edit(
            &human,
            "shared.txt",
            "human owns this line\n",
            "Human conflicting edit",
        )?;
        self.git(&human, &["push", "origin", "HEAD:refs/heads/main"])?;
        self.git(&authority, &["fetch", "origin"])?;
        let conflict = self.raw(
            &authority,
            &["merge-tree", "--write-tree", &trunk, &human_conflict],
            None,
        )?;
        self.check(
            conflict.status.code() == Some(1),
            "genuine conflict returns Git conflict exit 1",
        )?;
        let conflict_text = String::from_utf8_lossy(&conflict.stdout);
        self.check(
            conflict_text.contains("CONFLICT") && conflict_text.contains("shared.txt"),
            "genuine conflict identifies shared.txt",
        )?;
        let owner = self.owner.clone();
        conflict_owner(&owner)?;
        writeln!(self.log, "CONFLICT owner={owner} path=shared.txt trunk={trunk} upstream={human_conflict} last_synced={upstream}").map_err(|e| e.to_string())?;
        let current = self.git(&authority, &["rev-parse", "main"])?;
        let recorded = self.git(&authority, &["rev-parse", "refs/mai/upstream"])?;
        self.check(
            current == trunk && recorded == upstream,
            "conflict preserves trunk and last synced upstream",
        )?;

        self.git(&agent, &["fetch", "origin"])?;
        self.git(&agent, &["checkout", "-B", "work", "origin/main"])?;
        let independent = self.edit(
            &agent,
            "independent.txt",
            "safe local work\n",
            "Independent work during conflict",
        )?;
        self.git(
            &agent,
            &["push", "origin", "HEAD:refs/mai/submitted/independent"],
        )?;
        expected.insert("independent.txt".into(), "safe local work\n".into());
        let independent_tree = self.merge(&authority, &trunk, &independent)?;
        rev += 1;
        trunk = self.land(
            &authority,
            &independent_tree,
            &[&trunk],
            rev,
            "Independent landing",
            &expected,
        )?;
        let actual_upstream = self.git(&forge, &["rev-parse", "main"])?;
        let recorded = self.git(&authority, &["rev-parse", "refs/mai/upstream"])?;
        self.check(
            actual_upstream == human_conflict && recorded == upstream,
            "independent landing does not mutate human main or advance sync base",
        )?;
        let first_parent = self.git(
            &authority,
            &["rev-list", "--first-parent", &format!("{import}..{trunk}")],
        )?;
        let commits: Vec<_> = first_parent.lines().collect();
        self.check(
            commits.len() == rev as usize,
            "acceptance ledger contains exactly nine post-import revisions",
        )?;
        for (offset, commit) in commits.iter().enumerate() {
            let message = self.git(&authority, &["show", "-s", "--format=%B", commit])?;
            self.check(
                revision(&message)? == rev - offset as u32,
                "first-parent trailers are contiguous without recounting for assignment",
            )?;
        }
        writeln!(
            self.log,
            "MODE PASS {mode} cycles=3 revisions={rev} owner={owner} real_forge_api=pending"
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure(
        args.len() == 3 && args[1] == "run",
        "usage: s5 run <new-root>",
    )?;
    let root = PathBuf::from(&args[2]);
    ensure(!root.exists(), "fixture root must be new")?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    fs::write(root.join("empty-config"), "").map_err(|e| e.to_string())?;
    let owner =
        std::env::var("S5_CONFLICT_OWNER").unwrap_or_else(|_| "fixture-upstream-owner".into());
    conflict_owner(&owner)?;
    let mut harness = Harness {
        log: File::create(root.join("commands.log")).map_err(|e| e.to_string())?,
        root,
        assertions: 0,
        owner,
    };
    let started = Instant::now();
    for mode in ["squash", "rebase"] {
        harness.mode(mode)?;
    }
    println!(
        "PASS assertions={} cycles=6 modes=squash,rebase elapsed_ms={} platform={}-{} GitHub_API=pending",
        harness.assertions,
        started.elapsed().as_millis(),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_windows_git_paths() {
        assert_eq!(
            windows_git_path(r"\\?\C:\fixture dir\forge.git"),
            "C:/fixture dir/forge.git"
        );
        assert_eq!(
            windows_git_path(r"C:\fixture dir\forge.git"),
            "C:/fixture dir/forge.git"
        );
    }
    #[test]
    fn exact_gate_accepts_only_complete_expected_tree() {
        let expected = Files::from([("a".into(), "ok\n".into())]);
        assert!(gate(&expected, &expected).is_ok());
        assert!(gate(&Files::new(), &expected).is_err());
        let mut extra = expected.clone();
        extra.insert("unexpected".into(), "data".into());
        assert!(gate(&extra, &expected).is_err());
        let changed = Files::from([("a".into(), "wrong\n".into())]);
        assert!(gate(&changed, &expected).is_err());
    }
    #[test]
    fn invalid_content_fails_even_when_expected() {
        let invalid = Files::from([("a".into(), "INVALID\n".into())]);
        assert!(gate(&invalid, &invalid).unwrap_err().contains("rejected"));
    }
    #[test]
    fn revision_trailers_reject_missing_duplicate_and_malformed() {
        assert_eq!(revision("sync\n\nMai-Revision: r42\n").unwrap(), 42);
        for message in [
            "none",
            "Mai-Revision: r1\nMai-Revision: r2",
            "Mai-Revision: rx",
            "Mai-Revision: r-1",
        ] {
            assert!(revision(message).is_err());
        }
    }
    #[test]
    fn conflict_requires_explicit_single_line_owner() {
        assert_eq!(conflict_owner("fixture-owner").unwrap(), "fixture-owner");
        for owner in ["", "  ", "owner\nother"] {
            assert!(conflict_owner(owner).is_err());
        }
    }
}
