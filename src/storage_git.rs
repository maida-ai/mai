//! Git subprocess adapter. Domain code does not shell out directly to Git.
use anyhow::{Result, bail};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
#[derive(Clone, Debug)]
pub struct Git {
    pub bare: PathBuf,
}
pub fn execute(mut cmd: Command, data: &[u8]) -> Result<Vec<u8>> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Disable repository-controlled hooks and external diff drivers. Filters and
    // local Git configuration still belong to the trusted-local security boundary.
    let mut child = cmd.spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let bytes = data.to_vec();
    // Feed concurrently to avoid a pipe deadlock on large index inputs.
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output()?;
    let written = writer
        .join()
        .map_err(|_| anyhow::anyhow!("Git input writer panicked"))?;
    if !output.status.success() {
        bail!(
            "Git command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    written?;
    Ok(output.stdout)
}
pub fn command() -> Command {
    let mut c = Command::new("git");
    c.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.fsync=committed",
        "-c",
        "core.fsyncMethod=fsync",
    ]);
    // Prevent inherited Git location/index settings from redirecting operations.
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        c.env_remove(key);
    }
    c
}
impl Git {
    pub fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        self.input(args, &[], &[])
    }
    pub fn input(&self, args: &[&str], data: &[u8], env: &[(&str, &str)]) -> Result<Vec<u8>> {
        let mut c = command();
        c.arg("--git-dir")
            .arg(&self.bare)
            .args(args)
            .envs(env.iter().copied());
        execute(c, data)
    }
    pub fn workspace(&self, path: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let mut c = command();
        c.arg("-C").arg(path).args(args);
        execute(c, &[])
    }
    pub fn text(&self, args: &[&str]) -> Result<String> {
        text(self.run(args)?)
    }
    pub fn wc_text(&self, path: &Path, args: &[&str]) -> Result<String> {
        text(self.workspace(path, args)?)
    }
    pub fn publish(
        &self,
        current: &str,
        commit: &str,
        receipt_ref: &str,
        old_receipt: &str,
    ) -> Result<()> {
        // Git's reference transaction publishes the head and recovery receipt together.
        let input = format!(
            "start\nupdate refs/heads/main {commit} {current}\nupdate {receipt_ref} {commit} {old_receipt}\nprepare\ncommit\n"
        );
        self.input(&["update-ref", "--stdin"], input.as_bytes(), &[])?;
        Ok(())
    }
}
pub fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?.trim_end().to_owned())
}
