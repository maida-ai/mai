//! Separate-process tests exercise actual OS ownership and CLI startup.
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn cli(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mai"))
        .args(args)
        .arg(root)
        .output()
        .unwrap()
}
struct EngineProcess(Child);
impl std::ops::Deref for EngineProcess {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for EngineProcess {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for EngineProcess {
    fn drop(&mut self) {
        // Test-owned process handle only: failed assertions cannot orphan a
        // foreground engine whose IPC may be the feature under investigation.
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
struct Fixture {
    _temp: TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("authority space-é");
        let out = cli(&["init"], &root);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Self { _temp: temp, root }
    }
    fn diagnostics(&self) -> String {
        ["foreground.log", "engine.log"]
            .iter()
            .map(|name| {
                let text =
                    std::fs::read_to_string(self.root.join(".mai").join(name)).unwrap_or_default();
                format!("{name}: {text}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn start(&self) -> EngineProcess {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join(".mai/foreground.log"))
            .unwrap();
        EngineProcess(
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["serve", "--foreground"])
                .arg(&self.root)
                .stdout(Stdio::null())
                .stderr(log)
                .spawn()
                .unwrap(),
        )
    }
    fn ready(&self) -> serde_json::Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let out = cli(&["status", "--json", "--no-autostart"], &self.root);
            if out.status.success() {
                return serde_json::from_slice(&out.stdout).unwrap();
            }
            assert!(
                Instant::now() < deadline,
                "engine never ready: {}\n{}",
                String::from_utf8_lossy(&out.stderr),
                self.diagnostics()
            );
            thread::sleep(Duration::from_millis(30));
        }
    }
    fn stop(&self) {
        let out = cli(&["stop"], &self.root);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = cli(&["stop"], &self.root);
    }
}
fn wait(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "engine failed to exit");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn duplicate_start_cannot_steal_owner_and_stop_is_graceful() {
    let f = Fixture::new();
    let mut owner = f.start();
    let status = f.ready();
    assert_eq!(status["pid"], owner.id());
    assert_eq!(status["protocol_version"], 1);
    assert_eq!(status["registration_available"], false);
    std::fs::write(f.root.join(".mai/engine.pid"), "999999999\n").unwrap();
    let duplicate = cli(&["serve", "--foreground"], &f.root);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("owned"));
    assert_eq!(f.ready()["pid"], owner.id());
    f.stop();
    assert!(wait(&mut owner).success());
    assert!(f.root.join(".mai/engine.lock").is_file());
    assert!(!f.root.join(".mai/engine.pid").exists());
}

#[test]
fn killed_engine_restarts_without_changing_git_refs() {
    let f = Fixture::new();
    let refs = || {
        Command::new("git")
            .arg("--git-dir")
            .arg(f.root.join("authority.git"))
            .args(["show-ref"])
            .output()
            .unwrap()
            .stdout
    };
    let before = refs();
    let mut owner = f.start();
    f.ready();
    owner.kill().unwrap();
    owner.wait().unwrap();
    let mut restarted = f.start();
    f.ready();
    assert_eq!(before, refs());
    f.stop();
    assert!(wait(&mut restarted).success());
}

#[test]
fn concurrent_autostart_converges_to_one_engine() {
    let f = Fixture::new();
    let clients: Vec<_> = (0..4)
        .map(|index| {
            let stdout = f.root.join(format!("client-{index}.out"));
            let stderr = f.root.join(format!("client-{index}.err"));
            let child = Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["status", "--json"])
                .arg(&f.root)
                .stdout(std::fs::File::create(&stdout).unwrap())
                .stderr(std::fs::File::create(&stderr).unwrap())
                .spawn()
                .unwrap();
            (EngineProcess(child), stdout, stderr)
        })
        .collect();
    let values: Vec<serde_json::Value> = clients
        .into_iter()
        .map(|(mut child, stdout, stderr)| {
            let status = wait(&mut child);
            assert!(
                status.success(),
                "{}\n{}",
                std::fs::read_to_string(stderr).unwrap(),
                f.diagnostics()
            );
            serde_json::from_slice(&std::fs::read(stdout).unwrap()).unwrap()
        })
        .collect();
    assert!(values.iter().all(|v| v["pid"] == values[0]["pid"]));
    f.stop();
}

#[test]
fn autostart_closes_captured_output_before_engine_shutdown() {
    use std::io::Read;
    let f = Fixture::new();
    let mut child = EngineProcess(
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["status", "--json"])
            .arg(&f.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = thread::spawn(move || {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let result = stdout
            .read_to_end(&mut out)
            .and_then(|_| stderr.read_to_end(&mut err));
        let _ = tx.send((result, out, err));
    });
    let exit = wait(&mut child);
    let captured = rx.recv_timeout(Duration::from_secs(2));
    // Stop the engine before asserting a failed EOF check, so a broken
    // inherited handle cannot hang the entire test suite or obscure failures.
    let stopped = cli(&["stop"], &f.root);
    if captured.is_err() {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    if reader.is_finished() {
        reader.join().unwrap();
    }
    assert!(
        captured.is_ok(),
        "background engine inherited the CLI output pipe\n{}",
        f.diagnostics()
    );
    let (read, out, err) = captured.unwrap();
    read.unwrap();
    assert!(
        exit.success(),
        "{}\n{}",
        String::from_utf8_lossy(&err),
        f.diagnostics()
    );
    let status: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(status["state"], "ready");
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
}

#[test]
fn unavailable_and_corrupt_authority_fail_actionably() {
    let f = Fixture::new();
    let out = cli(&["status", "--no-autostart"], &f.root);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("serve"));
    std::fs::write(f.root.join(".mai/config.toml"), "format_version = 999\n").unwrap();
    let start = Instant::now();
    let out = cli(&["status"], &f.root);
    assert!(!out.status.success());
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(String::from_utf8_lossy(&out.stderr).contains("engine.log"));
    assert!(f.root.join("authority.git").is_dir());
}

#[test]
fn corrupt_database_does_not_become_ready() {
    let f = Fixture::new();
    std::fs::write(f.root.join(".mai/state.db"), "not SQLite").unwrap();
    let out = cli(&["serve", "--foreground"], &f.root);
    assert!(!out.status.success());
    assert!(!cli(&["status", "--no-autostart"], &f.root).status.success());
}

#[test]
fn malformed_and_idle_clients_cannot_stop_or_hang_engine() {
    let f = Fixture::new();
    let mut owner = f.start();
    f.ready();
    for payload in [
        b"{\"protocol_version\":999,\"action\":\"stop\"}\n".as_slice(),
        b"garbage\n".as_slice(),
        &[b'x'; 4097],
    ] {
        let mut stream = raw_client(&f.root);
        stream.write_all(payload).unwrap();
    }
    let mut idle = raw_client(&f.root);
    idle.write_all(b"{\"protocol_version\":1").unwrap();
    let start = Instant::now();
    assert_eq!(f.ready()["pid"], owner.id());
    assert!(start.elapsed() < Duration::from_secs(8));
    drop(idle);
    f.stop();
    assert!(wait(&mut owner).success());
}

#[cfg(unix)]
fn raw_client(root: &Path) -> Box<dyn std::io::Write> {
    Box::new(std::os::unix::net::UnixStream::connect(root.join(".mai/engine.sock")).unwrap())
}
#[cfg(windows)]
fn raw_client(root: &Path) -> Box<dyn std::io::Write> {
    use std::{
        hash::{Hash, Hasher},
        os::windows::ffi::OsStrExt,
    };
    let local = root.canonicalize().unwrap().join(".mai");
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    local
        .as_os_str()
        .encode_wide()
        .collect::<Vec<_>>()
        .hash(&mut hash);
    let name = format!(r"\\.\pipe\mai-{:016x}", hash.finish());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
        {
            return Box::new(file);
        }
        assert!(
            Instant::now() < deadline,
            "test could not connect named pipe"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn unrelated_endpoint_or_diagnostic_symlinks_are_preserved() {
    use std::os::unix::fs::symlink;
    for name in ["engine.pid", "engine.lock", "engine.sock", "engine.log"] {
        let f = Fixture::new();
        let original = f.root.join("unrelated.txt");
        std::fs::write(&original, b"preserve me").unwrap();
        symlink(&original, f.root.join(".mai").join(name)).unwrap();
        let args = if name == "engine.log" {
            vec!["status"]
        } else {
            vec!["serve", "--foreground"]
        };
        let out = cli(&args, &f.root);
        assert!(!out.status.success(), "accepted {name} symlink");
        assert_eq!(std::fs::read(&original).unwrap(), b"preserve me");
        assert!(
            std::fs::symlink_metadata(f.root.join(".mai").join(name))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_authority_round_trips_through_init_status_and_stop() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let root = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"authority-\xff".to_vec()));
    let init = cli(&["init", "--json"], &root);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let init: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    let path = &init["authority"];
    assert_eq!(path["encoding"], "unix_bytes");
    assert_eq!(
        STANDARD.decode(path["data"].as_str().unwrap()).unwrap(),
        root.canonicalize().unwrap().as_os_str().as_bytes()
    );
    let f = Fixture { _temp: temp, root };
    let mut owner = f.start();
    assert_eq!(f.ready()["authority"], *path);
    let stop = cli(&["stop", "--json"], &f.root);
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    let stop: serde_json::Value = serde_json::from_slice(&stop.stdout).unwrap();
    assert_eq!(stop["authority"], *path);
    assert!(wait(&mut owner).success());
}
