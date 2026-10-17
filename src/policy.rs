//! Strict trunk-owned policy and exact-candidate identities for manual validation.
use crate::{authority::Authority, native_path::NativePath};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPolicy {
    pub program: String,
    pub script: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub executable_dependencies: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub timeout_ms: u64,
    pub output_tail_bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatorPolicy {
    pub slots: usize,
    pub port_base: u16,
    pub ports_per_slot: u16,
    #[serde(default)]
    pub caches: Vec<PathBuf>,
    pub bootstrap: Option<CommandPolicy>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspacePolicy {
    pub bootstrap: Option<CommandPolicy>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionPolicy {
    #[serde(default)]
    pub protected: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    pub check: CommandPolicy,
    pub validators: ValidatorPolicy,
    #[serde(default)]
    pub workspaces: WorkspacePolicy,
    #[serde(default)]
    pub policy: ProtectionPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub schema_version: u32,
    pub authority: NativePath,
    pub candidate: String,
    pub tree: String,
    pub base: String,
    pub policy: String,
    pub submitted_identity: Option<String>,
}
pub struct TrustedPolicy {
    pub config: Policy,
    pub binding: Binding,
    pub protected_changes: Vec<NativePath>,
    checkout: tempfile::TempDir,
}
impl TrustedPolicy {
    pub fn load(authority: &Authority, candidate: &str) -> Result<Self> {
        ensure!(
            !candidate.starts_with('-'),
            "candidate must be a commit identity or revision"
        );
        let repo = authority.git_dir();
        let base = git(
            &repo,
            &["rev-parse", "--verify", "refs/heads/trunk^{commit}"],
        )?;
        let candidate = git(
            &repo,
            &["rev-parse", "--verify", &format!("{candidate}^{{commit}}")],
        )?;
        let tree = git(&repo, &["rev-parse", &format!("{candidate}^{{tree}}")])?;
        let checkout = tempfile::Builder::new()
            .prefix("mai-policy-")
            .tempdir_in(authority.root().join(".mai"))?;
        command_git(&[
            "clone".into(),
            "--no-local".into(),
            "--no-hardlinks".into(),
            "--no-checkout".into(),
            "--".into(),
            crate::repository_scope::external_git_path(&repo),
            crate::repository_scope::external_git_path(checkout.path()),
        ])?;
        git(
            checkout.path(),
            &["-c", "core.hooksPath=", "checkout", "--detach", &base],
        )?;
        checked_git_file(&authority.git_dir(), &base, "mai.toml")?;
        let policy_path = checked_file(checkout.path(), "mai.toml")?;
        let config: Policy = toml::from_str(&fs::read_to_string(policy_path)?)
            .context("invalid trusted mai.toml policy")?;
        ensure!(
            config.version == 1,
            "unsupported policy version; no migration is supported"
        );
        ensure!(
            config.validators.slots > 0 && config.validators.ports_per_slot > 0,
            "validator slot and port settings must be positive"
        );
        let commands = std::iter::once(&config.check)
            .chain(config.validators.bootstrap.iter())
            .chain(config.workspaces.bootstrap.iter());
        let mut protected = BTreeSet::from(["mai.toml".to_owned()]);
        for cmd in commands {
            ensure!(
                cmd.timeout_ms > 0
                    && cmd.output_tail_bytes > 0
                    && cmd.output_tail_bytes <= 1_048_576,
                "command requires positive timeout and output tail at most 1048576 bytes"
            );
            let program = Path::new(&cmd.program);
            ensure!(
                !cmd.program.is_empty()
                    && (program.is_absolute() || program.components().count() == 1)
                    && !cmd.program.starts_with('-'),
                "program must be an absolute host tool or bare executable name"
            );
            for path in std::iter::once(&cmd.script)
                .chain(cmd.executable_dependencies.iter())
                .chain(cmd.inputs.iter())
            {
                checked_git_file(&authority.git_dir(), &base, path)?;
                checked_file(checkout.path(), path)?;
                protected.insert(path.clone());
            }
        }
        for path in &config.validators.caches {
            relative_path(path)?;
        }
        for pattern in &config.policy.protected {
            validate_pattern(pattern)?;
        }
        let raw = git_bytes(
            authority.git_dir().as_path(),
            &[
                "diff",
                "--no-renames",
                "--name-only",
                "-z",
                &base,
                &candidate,
            ],
        )?;
        let mut protected_changes = Vec::new();
        for bytes in raw.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let path = native_git_path(bytes)?;
            if protected.iter().any(|p| p.as_bytes() == bytes)
                || config
                    .policy
                    .protected
                    .iter()
                    .any(|pattern| matches_pattern(pattern, bytes))
            {
                protected_changes.push(NativePath::from_path(&path)?);
            }
        }
        // The base commit binds the full trusted policy tree, including all executable dependencies.
        let binding = Binding {
            schema_version: 1,
            authority: NativePath::from_path(authority.root())?,
            candidate,
            tree,
            base: base.clone(),
            policy: base,
            submitted_identity: None,
        };
        Ok(Self {
            config,
            binding,
            protected_changes,
            checkout,
        })
    }
    pub fn root(&self) -> &Path {
        self.checkout.path()
    }
    pub fn script(&self, command: &CommandPolicy) -> Result<PathBuf> {
        checked_file(self.root(), &command.script)
    }
    pub fn check_candidate_input(&self, path: &str) -> Result<()> {
        checked_git_file(
            &self.binding.authority.as_path().join("authority.git"),
            &self.binding.candidate,
            path,
        )
    }
}
pub fn relative_path(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "policy path must be a normalized repository-relative path"
    );
    // Components and Path equality normalize interior `.` and separators. Compare
    // the original native spelling to a reconstructed slash-separated spelling
    // before using strings as authoritative protection keys.
    let mut normalized = OsString::new();
    for component in path.components() {
        if !normalized.is_empty() {
            normalized.push("/");
        }
        normalized.push(component.as_os_str());
    }
    ensure!(
        path.as_os_str() == normalized.as_os_str(),
        "policy path must use canonical repository-relative spelling"
    );
    ensure!(
        !path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git")),
        "policy paths cannot enter .git"
    );
    ensure!(
        !path.to_string_lossy().contains('\\'),
        "backslash paths are unsupported"
    );
    Ok(())
}
/// Git modes and exact tree spelling remain authoritative on filesystems that
/// materialize symlinks as ordinary files or compare names without case.
fn checked_git_file(repo: &Path, tree: &str, path: &str) -> Result<()> {
    relative_path(Path::new(path))?;
    let entries = git_bytes(repo, &["ls-tree", "-z", tree, "--", path])?;
    let mut entries = entries
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    let entry = entries
        .next()
        .context("trusted Git path is missing or not spelled exactly")?;
    ensure!(
        entries.next().is_none(),
        "trusted Git path must name exactly one file"
    );
    let (identity, name) = entry.split_at(
        entry
            .iter()
            .position(|byte| *byte == b'\t')
            .context("invalid Git tree record")?,
    );
    ensure!(
        name.get(1..) == Some(path.as_bytes()),
        "trusted Git path is not spelled exactly"
    );
    ensure!(
        identity.starts_with(b"100644 blob ") || identity.starts_with(b"100755 blob "),
        "trusted Git path must have a regular-file mode"
    );
    Ok(())
}
pub fn checked_file(root: &Path, path: &str) -> Result<PathBuf> {
    relative_path(Path::new(path))?;
    let mut current = root.to_owned();
    for part in Path::new(path).components() {
        current.push(part);
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "policy inputs must not contain symlinks: {path}"
        );
    }
    ensure!(
        current.is_file(),
        "policy input must be a regular file: {path}"
    );
    Ok(current)
}
fn validate_pattern(pattern: &str) -> Result<()> {
    let prefix = pattern.strip_suffix("/**").unwrap_or(pattern);
    ensure!(
        !prefix.contains(['*', '?', '[', ']']),
        "only exact protected paths and directory/** are supported"
    );
    relative_path(Path::new(prefix))
}
fn matches_pattern(pattern: &str, path: &[u8]) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        path == prefix.as_bytes()
            || path
                .strip_prefix(prefix.as_bytes())
                .is_some_and(|s| s.starts_with(b"/"))
    } else {
        pattern.as_bytes() == path
    }
}
pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_bytes(repo, args)?)?.trim().to_owned())
}
pub(crate) fn safe_git() -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            cmd.env_remove(key);
        }
    }
    cmd.args([
        "--literal-pathspecs",
        "-c",
        "core.hooksPath=",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "protocol.allow=never",
    ])
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
    .env("GIT_NO_REPLACE_OBJECTS", "1")
    .env("GIT_NO_LAZY_FETCH", "1")
    .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}
fn git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = safe_git()
        .arg("-C")
        .arg(crate::repository_scope::external_git_path(repo))
        .args(args)
        .output()?;
    ensure!(
        out.status.success(),
        "Git policy operation failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out.stdout)
}
fn command_git(args: &[OsString]) -> Result<()> {
    let out = safe_git()
        .args(["-c", "protocol.file.allow=always"])
        .args(args)
        .output()?;
    ensure!(
        out.status.success(),
        "trusted policy clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}
pub fn host_tool(program: &str) -> Result<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.canonicalize().context("missing host tool");
    }
    ensure!(
        path.components().count() == 1,
        "host tool must be absolute or bare name"
    );
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|p| p.is_absolute())
    {
        let path = directory.join(program);
        if path.is_file() {
            return Ok(path.canonicalize()?);
        }
        #[cfg(windows)]
        for extension in ["exe", "cmd", "bat"] {
            let path = path.with_extension(extension);
            if path.is_file() {
                return Ok(path.canonicalize()?);
            }
        }
    }
    anyhow::bail!("host tool not found in absolute PATH entries: {program}")
}
#[cfg(unix)]
fn native_git_path(bytes: &[u8]) -> Result<PathBuf> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    Ok(OsString::from_vec(bytes.to_vec()).into())
}
#[cfg(windows)]
fn native_git_path(bytes: &[u8]) -> Result<PathBuf> {
    Ok(std::str::from_utf8(bytes)?.into())
}

impl TrustedPolicy {
    pub fn command_spec(
        &self,
        command: &CommandPolicy,
        cwd: &Path,
    ) -> Result<crate::process::RunSpec> {
        let mut env = std::collections::BTreeMap::new();
        env.insert("MAI_POLICY_ROOT".into(), external_command_path(self.root()));
        env.insert("MAI_CANDIDATE_ROOT".into(), external_command_path(cwd));
        let mut args: Vec<OsString> = vec![external_command_path(&self.script(command)?)];
        args.extend(command.args.iter().map(OsString::from));
        Ok(crate::process::RunSpec {
            program: host_tool(&command.program)?.into_os_string(),
            args,
            cwd: self.root().to_owned(),
            env,
            timeout: std::time::Duration::from_millis(command.timeout_ms),
            output_tail_bytes: command.output_tail_bytes,
        })
    }
}

/// Native files and JSON retain canonical paths. Shell-facing policy arguments
/// use ordinary drive paths with slashes so Git for Windows sh can consume them.
pub fn external_command_path(path: &Path) -> OsString {
    let path = crate::repository_scope::external_git_path(path);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        OsString::from_wide(
            &path
                .encode_wide()
                .map(|unit| if unit == 92 { 47 } else { unit })
                .collect::<Vec<_>>(),
        )
    }
    #[cfg(not(windows))]
    {
        path
    }
}

#[cfg(all(test, windows))]
mod command_path_tests {
    use super::*;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[test]
    fn shell_paths_preserve_native_utf16_units() {
        let units = [92, 92, 63, 92, 67, 58, 92, 116, 109, 112, 92, 0xd800];
        let original = PathBuf::from(OsString::from_wide(&units));
        assert_eq!(
            external_command_path(&original)
                .encode_wide()
                .collect::<Vec<_>>(),
            [67, 58, 47, 116, 109, 112, 47, 0xd800]
        );
    }
}
