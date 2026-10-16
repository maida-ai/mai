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
struct Fixture {
    _temp: TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("authority");
        let out = cli(&["init"], &root);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Self { _temp: temp, root }
    }
    fn start(&self) -> Child {
        Command::new(env!("CARGO_BIN_EXE_mai"))
            .args(["serve", "--foreground"])
            .arg(&self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
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
                "engine never ready: {}",
                String::from_utf8_lossy(&out.stderr)
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
    let mut clients: Vec<_> = (0..4)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_mai"))
                .args(["status", "--json"])
                .arg(&f.root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let values: Vec<serde_json::Value> = clients
        .drain(..)
        .map(|c| {
            let out = c.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice(&out.stdout).unwrap()
        })
        .collect();
    assert!(values.iter().all(|v| v["pid"] == values[0]["pid"]));
    f.stop();
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
