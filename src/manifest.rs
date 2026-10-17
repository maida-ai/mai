//! Read-only, deterministic M1 file manifests from immutable Git trees.
//!
//! This is computation evidence, not a received/landed record or permission to
//! land. Notes, generation associations and the rebuildable index are separate.
use crate::repository_scope::external_git_path;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};

/// A repository-relative Git byte path, independent of host path conventions.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FileUnit(Vec<u8>);

impl FileUnit {
    pub fn from_git_path(path: &[u8]) -> Result<Self> {
        ensure!(
            !path.is_empty() && !path.contains(&0),
            "empty or NUL Git path"
        );
        ensure!(
            path.split(|b| *b == b'/')
                .all(|part| !part.is_empty() && part != b"." && part != b".."),
            "noncanonical repository-relative Git path"
        );
        Ok(Self(path.to_vec()))
    }
    pub fn as_git_path(&self) -> &[u8] {
        &self.0
    }
    pub fn canonical(&self) -> String {
        const HEX: &[u8] = b"0123456789ABCDEF";
        let mut encoded = String::from("file:");
        for &byte in &self.0 {
            if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
                encoded.push(byte as char);
            } else {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 15) as usize] as char);
            }
        }
        encoded
    }
}
impl TryFrom<String> for FileUnit {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        let encoded = value.strip_prefix("file:").context("expected file: unit")?;
        let mut bytes = Vec::new();
        let mut input = encoded.bytes();
        while let Some(byte) = input.next() {
            if byte == b'%' {
                let hi = input.next().context("truncated percent escape")?;
                let lo = input.next().context("truncated percent escape")?;
                let digit = |b: u8| match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'A'..=b'F' => Ok(b - b'A' + 10),
                    _ => bail!("invalid uppercase percent escape"),
                };
                bytes.push(digit(hi)? * 16 + digit(lo)?);
            } else {
                bytes.push(byte);
            }
        }
        let unit = Self::from_git_path(&bytes)?;
        ensure!(unit.canonical() == value, "noncanonical file unit");
        Ok(unit)
    }
}
impl From<FileUnit> for String {
    fn from(value: FileUnit) -> Self {
        value.canonical()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Added,
    Modified,
    Deleted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    File,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackReason {
    #[serde(rename = "mvp-file-only")]
    MvpFileOnly,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub unit: FileUnit,
    pub operation: Operation,
    pub granularity: Granularity,
    /// M1 deliberately runs no language parser.
    pub parser: Option<String>,
    pub fallback_reason: FallbackReason,
}

/// Versioned computation output. The tree pair is exact; callers must bind it
/// to their received generation or final landed object before persisting notes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ManifestWire", into = "ManifestWire")]
pub struct FileManifest {
    pub base_tree: String,
    pub result_tree: String,
    pub entries: Vec<FileEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestWire {
    schema_version: u32,
    base_tree: String,
    result_tree: String,
    entries: Vec<FileEntry>,
}
impl TryFrom<ManifestWire> for FileManifest {
    type Error = anyhow::Error;
    fn try_from(wire: ManifestWire) -> Result<Self> {
        ensure!(
            wire.schema_version == 1,
            "unsupported manifest schema version"
        );
        let manifest = Self {
            base_tree: wire.base_tree,
            result_tree: wire.result_tree,
            entries: wire.entries,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}
impl From<FileManifest> for ManifestWire {
    fn from(value: FileManifest) -> Self {
        Self {
            schema_version: 1,
            base_tree: value.base_tree,
            result_tree: value.result_tree,
            entries: value.entries,
        }
    }
}
impl FileManifest {
    pub fn validate(&self) -> Result<()> {
        for oid in [&self.base_tree, &self.result_tree] {
            ensure!(
                oid.len() == 40
                    && oid.bytes().any(|b| b != b'0')
                    && oid
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid full tree object ID"
            );
        }
        ensure!(
            self.entries.iter().all(|entry| entry.parser.is_none()),
            "M1 manifests cannot claim parser evidence"
        );
        ensure!(
            self.entries
                .windows(2)
                .all(|pair| pair[0].unit < pair[1].unit),
            "manifest entries must be unique and sorted by raw path bytes"
        );
        Ok(())
    }
    /// Value of Mai-Objects, with delimiter-safe units and no truncated list.
    pub fn trailer_value(&self) -> Result<String> {
        self.validate()?;
        Ok(match self.entries.len() {
            0 => "none".to_owned(),
            1..=20 => self
                .entries
                .iter()
                .map(|e| e.unit.canonical())
                .collect::<Vec<_>>()
                .join(","),
            count => format!("count={count};note=refs/notes/mai-objects@self"),
        })
    }
}

/// Compute from two commits or trees in a bare or ordinary repository's Git
/// directory. No checkout, rename heuristic, text converter or external diff runs.
pub fn compute(git_dir: &Path, base: &str, result: &str) -> Result<FileManifest> {
    let resolve = |object: &str| -> Result<String> {
        ensure!(
            !object.is_empty() && !object.starts_with('-'),
            "invalid tree revision"
        );
        let bytes = git(
            git_dir,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{object}^{{tree}}"),
            ],
        )?;
        Ok(String::from_utf8(bytes)
            .context("non-ASCII object ID")?
            .trim()
            .to_owned())
    };
    let base_tree = resolve(base)?;
    let result_tree = resolve(result)?;
    let output = git(
        git_dir,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-r",
            "-z",
            "--no-renames",
            "--no-relative",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
            &base_tree,
            &result_tree,
            "--",
        ],
    )?;
    let mut entries = Vec::new();
    if !output.is_empty() {
        ensure!(output.last() == Some(&0), "unterminated Git diff output");
        let mut fields = output[..output.len() - 1].split(|b| *b == 0);
        while let Some(status) = fields.next() {
            let operation = match status {
                b"A" => Operation::Added,
                b"M" | b"T" => Operation::Modified,
                b"D" => Operation::Deleted,
                _ => bail!("unsupported Git diff status"),
            };
            entries.push(FileEntry {
                unit: FileUnit::from_git_path(fields.next().context("missing Git diff path")?)?,
                operation,
                granularity: Granularity::File,
                parser: None,
                fallback_reason: FallbackReason::MvpFileOnly,
            });
        }
    }
    entries.sort_by(|a, b| a.unit.cmp(&b.unit));
    let manifest = FileManifest {
        base_tree,
        result_tree,
        entries,
    };
    manifest.validate()?;
    Ok(manifest)
}
fn git(git_dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    let output = command
        .arg("--git-dir")
        .arg(external_git_path(git_dir))
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("execute manifest Git plumbing")?;
    ensure!(
        output.status.success(),
        "manifest Git {} failed ({})",
        args[0],
        output.status
    );
    Ok(output.stdout)
}
