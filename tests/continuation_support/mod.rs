#![allow(dead_code)]

use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime},
};

pub fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

pub fn git_output(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(mai::repository_scope::external_git_path(root))
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_AUTHOR_NAME", "continuation-QA")
        .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
        .env("GIT_COMMITTER_NAME", "continuation-QA")
        .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
        .output()
        .unwrap()
}

pub fn git(root: &Path, args: &[&str]) -> String {
    success(git_output(root, args))
}

pub fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mai"))
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}

pub fn json(output: Output) -> Value {
    serde_json::from_str(&success(output)).unwrap()
}

pub struct Process(pub Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

pub fn wait(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "fixture process did not exit");
        thread::sleep(Duration::from_millis(20));
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Entry {
    Directory,
    File(Vec<u8>, u32, SystemTime),
    Symlink(PathBuf),
}

pub type Snapshot = BTreeMap<PathBuf, Entry>;

/// Include source Git metadata, ignored files, permissions and modification
/// times. Merely running Git status must not refresh the source index.
pub fn snapshot(root: &Path) -> Snapshot {
    fn collect(root: &Path, relative: &Path, result: &mut Snapshot) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            if metadata.file_type().is_symlink() {
                result.insert(path, Entry::Symlink(fs::read_link(entry.path()).unwrap()));
            } else if metadata.is_dir() {
                result.insert(path.clone(), Entry::Directory);
                collect(root, &path, result);
            } else {
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = u32::from(metadata.permissions().readonly());
                result.insert(
                    path,
                    Entry::File(
                        fs::read(entry.path()).unwrap(),
                        mode,
                        metadata.modified().unwrap(),
                    ),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    collect(root, Path::new(""), &mut result);
    result
}

pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub project: PathBuf,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub baseline: String,
    pub submitted: String,
    pub landed: String,
    pub trunk: String,
    pub suffix: String,
    pub change: String,
}

impl Fixture {
    pub fn new() -> Self {
        Self::with_bootstrap(None)
    }

    pub fn with_bootstrap(bootstrap: Option<&str>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let seed = temp.path().join("seed");
        let project = temp.path().join("authority");
        let source = temp.path().join("source");
        fs::create_dir(&seed).unwrap();
        git(&seed, &["init"]);
        fs::write(
            seed.join("gate.sh"),
            "test ! -f \"$MAI_CANDIDATE_ROOT/fail\"\n",
        )
        .unwrap();
        let mut policy = "version=1\n[check]\nprogram=\"sh\"\nscript=\"gate.sh\"\ntimeout_ms=5000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=41000\nports_per_slot=10\n".to_owned();
        if let Some(script) = bootstrap {
            fs::write(seed.join("bootstrap.sh"), script).unwrap();
            policy.push_str("[workspaces.bootstrap]\nprogram=\"sh\"\nscript=\"bootstrap.sh\"\ntimeout_ms=5000\noutput_tail_bytes=1024\n");
        }
        fs::write(seed.join("mai.toml"), policy).unwrap();
        fs::write(seed.join(".gitignore"), "ignored/\nignored.log\n").unwrap();
        fs::write(seed.join("accepted.txt"), "baseline\n").unwrap();
        fs::write(seed.join("local.txt"), "baseline local\n").unwrap();
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "continuation fixture policy"]);
        success(cli(&[
            "init".as_ref(),
            project.as_os_str(),
            "--from".as_ref(),
            seed.as_os_str(),
        ]));
        success(
            Command::new("git")
                .args(["-c", "core.autocrlf=false", "clone"])
                .arg(project.join("authority.git"))
                .arg(&source)
                .output()
                .unwrap(),
        );
        git(&source, &["config", "core.autocrlf", "false"]);
        git(&source, &["config", "mai.agent", "alice"]);
        git(
            &source,
            &[
                "remote",
                "add",
                "mai",
                project.join("authority.git").to_str().unwrap(),
            ],
        );
        let baseline = git(&source, &["rev-parse", "HEAD"]);
        let mut fixture = Self {
            destination: temp.path().join("continued"),
            temp,
            project,
            source,
            baseline,
            submitted: String::new(),
            landed: String::new(),
            trunk: String::new(),
            suffix: String::new(),
            change: String::new(),
        };
        let mut engine = fixture.start();
        success(cli(&[
            "receive".as_ref(),
            "enable".as_ref(),
            fixture.project.as_os_str(),
            "--max-input-bytes".as_ref(),
            "1048576".as_ref(),
            "--max-file-bytes".as_ref(),
            "65536".as_ref(),
            "--max-changed-files".as_ref(),
            "16".as_ref(),
        ]));
        fs::write(fixture.source.join("accepted.txt"), "submitted parent\n").unwrap();
        git(&fixture.source, &["add", "accepted.txt"]);
        git(&fixture.source, &["commit", "-m", "accepted parent"]);
        fixture.submitted = git(&fixture.source, &["rev-parse", "HEAD"]);
        fixture.change = format!("alice-{}", fixture.submitted);
        git(
            &fixture.source,
            &["push", "-o", "agent=alice", "origin", "HEAD:refs/for/main"],
        );
        let landed = fixture.integrate(&fixture.change);
        fixture.landed = landed["commit"].as_str().unwrap().into();
        let bob = fixture.temp.path().join("bob");
        success(
            Command::new("git")
                .args(["-c", "core.autocrlf=false", "clone"])
                .arg(fixture.project.join("authority.git"))
                .arg(&bob)
                .output()
                .unwrap(),
        );
        fs::write(bob.join("accepted.txt"), "later accepted trunk edit\n").unwrap();
        fs::write(bob.join("bob.txt"), "independent trunk content\n").unwrap();
        git(&bob, &["add", "."]);
        git(&bob, &["commit", "-m", "later trunk edit"]);
        let bob_commit = git(&bob, &["rev-parse", "HEAD"]);
        git(
            &bob,
            &["push", "-o", "agent=bob", "origin", "HEAD:refs/for/main"],
        );
        fixture.trunk = fixture.integrate(&format!("bob-{bob_commit}"))["commit"]
            .as_str()
            .unwrap()
            .into();
        fixture.stop(&mut engine);
        fixture.commit_local("local.txt", "local suffix\n", "unsubmitted local suffix");
        fixture.suffix = git(&fixture.source, &["rev-parse", "HEAD"]);
        git(
            &fixture.source,
            &["config", "qa.preserve", "source-only-value"],
        );
        git(
            &fixture.source,
            &["update-ref", "refs/qa/preserve", &fixture.submitted],
        );
        fs::create_dir(fixture.source.join("ignored")).unwrap();
        fs::write(
            fixture.source.join("ignored/cache"),
            b"precious ignored source bytes",
        )
        .unwrap();
        fixture
    }

    pub fn start(&self) -> Process {
        let log = self.temp.path().join("engine.log");
        let mut owner = Process(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["serve", "--foreground"])
                .arg(&self.project)
                .stdout(Stdio::null())
                .stderr(fs::File::create(&log).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !cli(&[
            "status".as_ref(),
            self.project.as_os_str(),
            "--no-autostart".as_ref(),
        ])
        .status
        .success()
        {
            assert!(
                owner.0.try_wait().unwrap().is_none(),
                "{}",
                fs::read_to_string(&log).unwrap()
            );
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        owner
    }

    pub fn stop(&self, owner: &mut Process) {
        success(cli(&["stop".as_ref(), self.project.as_os_str()]));
        assert!(wait(&mut owner.0).success());
    }

    pub fn integrate(&self, change: &str) -> Value {
        json(cli(&[
            "integrate".as_ref(),
            self.project.as_os_str(),
            change.as_ref(),
            "--mode".as_ref(),
            "textual".as_ref(),
            "--wait-seconds".as_ref(),
            "30".as_ref(),
        ]))
    }

    pub fn commit_local(&self, file: &str, contents: &str, message: &str) {
        fs::write(self.source.join(file), contents).unwrap();
        git(&self.source, &["add", file]);
        git(&self.source, &["commit", "-m", message]);
    }

    pub fn continue_to(&self, destination: &Path) -> Output {
        self.continue_generation(destination, &self.change, "1")
    }

    pub fn continue_default_to(&self, destination: &Path) -> Output {
        cli(&[
            "continue".as_ref(),
            self.project.as_os_str(),
            self.source.as_os_str(),
            destination.as_os_str(),
            "--change".as_ref(),
            self.change.as_ref(),
            "--generation".as_ref(),
            "1".as_ref(),
        ])
    }

    pub fn continue_generation(
        &self,
        destination: &Path,
        change: &str,
        generation: &str,
    ) -> Output {
        cli(&[
            "continue".as_ref(),
            self.project.as_os_str(),
            self.source.as_os_str(),
            destination.as_os_str(),
            "--change".as_ref(),
            change.as_ref(),
            "--generation".as_ref(),
            generation.as_ref(),
            "--skip-bootstrap".as_ref(),
        ])
    }

    pub fn assert_authority_unchanged(&self) {
        assert_eq!(
            git(&self.project.join("authority.git"), &["rev-parse", "trunk"]),
            self.trunk
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Continue may autostart only this disposable authority's engine.
        let _ = cli(&["stop".as_ref(), self.project.as_os_str()]);
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.project.join(".mai/engine.pid").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }
}
