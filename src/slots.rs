//! Engine-owned independent validator clones. Persistent claims fail closed after
//! interruption; an expired PID or an unlocked lock never proves cleanup.
use crate::{
    native_path::NativePath,
    process::{self, CancellationToken, ProcessOutcome, RunResult, RunSpec},
    repository_scope::external_git_path,
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotSettings {
    pub count: usize,
    pub port_base: u16,
    pub ports_per_slot: u16,
    pub caches: Vec<PathBuf>,
}
#[derive(Clone)]
pub struct SlotPool {
    root: PathBuf,
    source: PathBuf,
    settings: SlotSettings,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PoolRecord {
    version: u32,
    source: NativePath,
    settings: SlotSettings,
}
fn lock_is_contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
        || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
}
pub struct SlotLease {
    pool: SlotPool,
    index: usize,
    directory: PathBuf,
    clone: PathBuf,
    temporary: PathBuf,
    _lock: File,
    healthy: bool,
}
fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "engine directory must not be a symlink: {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "engine directory must not be a symlink: {}",
        path.display()
    );
    Ok(())
}
fn ordinary(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
fn git(cwd: &Path, arguments: &[&std::ffi::OsStr]) -> Result<String> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "-c".into(),
        "core.hooksPath=".into(),
        "-c".into(),
        "core.fsmonitor=false".into(),
    ];
    args.extend(arguments.iter().map(|argument| argument.to_os_string()));
    let result = process::run(
        &RunSpec {
            program: "git".into(),
            args,
            cwd: PathBuf::from(external_git_path(cwd)),
            env: [
                ("GIT_LITERAL_PATHSPECS".into(), "1".into()),
                ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
                (
                    "GIT_CONFIG_GLOBAL".into(),
                    if cfg!(windows) {
                        "NUL".into()
                    } else {
                        "/dev/null".into()
                    },
                ),
                ("GIT_NO_REPLACE_OBJECTS".into(), "1".into()),
            ]
            .into(),
            timeout: std::time::Duration::from_secs(60),
            output_tail_bytes: 65536,
        },
        &CancellationToken::default(),
    )?;
    ensure!(
        result.outcome == ProcessOutcome::Success,
        "slot Git {:?}: {}{}",
        result.outcome,
        result
            .cleanup_diagnostic
            .as_ref()
            .map(|diagnostic| format!("{diagnostic}; "))
            .unwrap_or_default(),
        result.stderr_tail
    );
    Ok(result.stdout_tail.trim().to_owned())
}
impl SlotPool {
    pub fn open(root: &Path, source: &Path, settings: SlotSettings) -> Result<Self> {
        ensure!(
            settings.count > 0 && settings.count <= 1024,
            "slot count must be 1..=1024"
        );
        ensure!(
            settings.ports_per_slot > 0 && settings.port_base > 0,
            "explicit nonzero slot port allocation required"
        );
        ensure!(
            usize::from(settings.port_base) + settings.count * usize::from(settings.ports_per_slot)
                <= usize::from(u16::MAX) + 1,
            "slot ports overflow"
        );
        for cache in &settings.caches {
            ensure!(ordinary(cache), "cache must be a relative ordinary path");
            ensure!(
                cache
                    .components()
                    .next()
                    .is_some_and(|part| !part.as_os_str().eq_ignore_ascii_case(".git")),
                "cache cannot include .git"
            );
        }
        for (index, cache) in settings.caches.iter().enumerate() {
            ensure!(
                !settings.caches[..index]
                    .iter()
                    .any(|other| cache.starts_with(other) || other.starts_with(cache)),
                "overlapping cache paths"
            );
        }
        let source = fs::canonicalize(source)?;
        let marker = root.join("pool.json");
        if root.exists() {
            ensure!(
                !fs::symlink_metadata(root)?.file_type().is_symlink(),
                "slot pool root cannot be a symlink"
            );
            ensure!(
                marker.is_file(),
                "refusing unowned slot root {}",
                root.display()
            );
            let record: PoolRecord = serde_json::from_slice(&fs::read(&marker)?)?;
            ensure!(record.version == 1, "unsupported slot pool version");
            ensure!(
                record.source.as_path() == source && record.settings == settings,
                "slot pool source/settings changed; use a fresh engine-owned pool"
            );
        } else {
            private_directory(root)?;
            let record = PoolRecord {
                version: 1,
                source: NativePath::from_path(&source)?,
                settings: settings.clone(),
            };
            let file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&marker)?;
            serde_json::to_writer(&file, &record)?;
            file.sync_all()?;
        }
        let root = fs::canonicalize(root)?;
        Ok(Self {
            root,
            source,
            settings,
        })
    }
    pub fn acquire(&self) -> Result<SlotLease> {
        let mut unavailable = Vec::new();
        for index in 0..self.settings.count {
            let directory = self.root.join(format!("slot-{index}"));
            private_directory(&directory)?;
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(directory.join("lock"))?;
            match lock.try_lock_exclusive() {
                Ok(()) => {}
                Err(error) if lock_is_contended(&error) => {
                    unavailable.push(format!("slot {index}: native lock contention ({error})"));
                    continue;
                }
                Err(error) => {
                    return Err(error).with_context(|| format!("lock validator slot {index}"));
                }
            }
            let claim = directory.join("occupied");
            if claim.exists() {
                unavailable.push(format!("slot {index}: occupied claim retained"));
                continue;
            }
            let marker = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&claim)?;
            marker.sync_all()?;
            let clone = directory.join("candidate");
            let temporary_root = std::env::temp_dir();
            let temporary = tempfile::Builder::new()
                .prefix("mai-validator-")
                .tempdir_in(temporary_root)?
                .keep();
            ensure!(
                !temporary
                    .ancestors()
                    .any(|ancestor| ancestor.join(".git").symlink_metadata().is_ok()),
                "validator temporary directory must be outside a repository"
            );
            private_directory(&temporary)?;
            fs::write(
                directory.join("temporary.json"),
                serde_json::to_vec(&NativePath::from_path(&temporary)?)?,
            )?;
            return Ok(SlotLease {
                pool: self.clone(),
                index,
                directory,
                clone,
                temporary,
                _lock: lock,
                healthy: true,
            });
        }
        bail!(
            "no validator slot available ({}); occupied/interrupted slots require trusted cleanup before reuse",
            unavailable.join("; ")
        )
    }
}
impl SlotLease {
    pub fn path(&self) -> &Path {
        &self.clone
    }
    pub fn temporary_path(&self) -> &Path {
        &self.temporary
    }
    pub fn slot(&self) -> usize {
        self.index
    }
    pub fn port_base(&self) -> u16 {
        (usize::from(self.pool.settings.port_base)
            + self.index * usize::from(self.pool.settings.ports_per_slot)) as u16
    }
    fn verify_cache_paths(&self) -> Result<()> {
        for cache in &self.pool.settings.caches {
            let mut path = self.clone.clone();
            for part in cache.components() {
                path.push(part);
                match fs::symlink_metadata(&path) {
                    Ok(metadata) => ensure!(
                        metadata.file_type().is_dir(),
                        "cache path component must be a directory, not a file or symlink: {}",
                        path.display()
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(())
    }
    fn clean(&self) -> Result<()> {
        if let Ok(metadata) = fs::symlink_metadata(&self.clone) {
            ensure!(
                metadata.file_type().is_dir(),
                "candidate clone root is not an owned directory"
            );
        }
        self.verify_cache_paths()?;
        if self.clone.exists() {
            prune(&self.clone, Path::new(""), &self.pool.settings.caches)?;
        }
        Ok(())
    }
    pub fn materialize(&mut self, candidate: &str) -> Result<()> {
        self.healthy = false;
        let external_source = external_git_path(&self.pool.source);
        let external_clone = external_git_path(&self.clone);
        ensure!(
            (candidate.len() == 40 || candidate.len() == 64)
                && candidate.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "candidate must be an exact full commit identity"
        );
        self.clean()?;
        // Rebuild Git metadata too: candidate code can poison .git/config,
        // attributes, hooks or sparse-checkout. Warmth is only explicit caches.
        let staging = self.directory.join("cache-staging");
        ensure!(
            !staging.exists(),
            "interrupted cache staging requires inspection"
        );
        private_directory(&staging)?;
        for (index, cache) in self.pool.settings.caches.iter().enumerate() {
            let path = self.clone.join(cache);
            if path.exists() {
                fs::rename(path, staging.join(index.to_string()))?;
            }
        }
        if self.clone.exists() {
            fs::remove_dir_all(&self.clone)?;
        }
        git(
            &self.directory,
            &[
                "clone".as_ref(),
                "--no-local".as_ref(),
                "--no-hardlinks".as_ref(),
                "--no-checkout".as_ref(),
                "--".as_ref(),
                external_source.as_os_str(),
                external_clone.as_os_str(),
            ],
        )?;
        git(
            &self.clone,
            &[
                "fetch".as_ref(),
                "--no-tags".as_ref(),
                "--".as_ref(),
                external_source.as_os_str(),
                candidate.as_ref(),
            ],
        )?;
        let resolved = git(
            &self.clone,
            &[
                "rev-parse".as_ref(),
                "--verify".as_ref(),
                "FETCH_HEAD^{commit}".as_ref(),
            ],
        )?;
        ensure!(
            resolved.eq_ignore_ascii_case(candidate),
            "slot fetched a different commit"
        );
        for cache in &self.pool.settings.caches {
            let tracked = git(
                &self.clone,
                &[
                    "ls-tree".as_ref(),
                    "-z".as_ref(),
                    "-r".as_ref(),
                    "--name-only".as_ref(),
                    candidate.as_ref(),
                    "--".as_ref(),
                    cache.as_os_str(),
                ],
            )?;
            ensure!(
                tracked.is_empty(),
                "configured cache overlaps candidate tracked content"
            );
        }
        git(
            &self.clone,
            &[
                "checkout".as_ref(),
                "--detach".as_ref(),
                "--force".as_ref(),
                candidate.as_ref(),
            ],
        )?;
        // Checkout can introduce symlinks/files at previously safe ancestors.
        // Validate every destination before any mkdir or restoration rename.
        self.verify_cache_paths()?;
        for (index, cache) in self.pool.settings.caches.iter().enumerate() {
            let saved = staging.join(index.to_string());
            if saved.exists() {
                let target = self.clone.join(cache);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(saved, target)?;
            }
        }
        fs::remove_dir(&staging)?;
        self.verify_cache_paths()?;
        self.healthy = true;
        Ok(())
    }
    pub fn run(&mut self, spec: &RunSpec, cancellation: &CancellationToken) -> Result<RunResult> {
        ensure!(self.healthy, "slot cleanup/materialization failed");
        self.healthy = false;
        let mut spec = spec.clone();
        spec.env
            .insert("MAI_SLOT".into(), self.slot().to_string().into());
        spec.env
            .insert("MAI_PORT_BASE".into(), self.port_base().to_string().into());
        for name in ["TMPDIR", "TMP", "TEMP"] {
            spec.env
                .insert(name.into(), self.temporary.as_os_str().to_owned());
        }
        let result = process::run(&spec, cancellation)?;
        self.healthy = result.outcome != ProcessOutcome::CleanupFailure;
        Ok(result)
    }
    /// No Drop release: interrupted/unreleased claims stay unavailable. Cleanup
    /// failures preserve the durable claim, even after its OS lock is released.
    pub fn release(mut self) -> Result<()> {
        ensure!(
            self.healthy,
            "unhealthy slot retained for trusted inspection"
        );
        self.healthy = false;
        self.clean()?;
        fs::remove_dir_all(&self.temporary)?;
        fs::remove_file(self.directory.join("temporary.json"))?;
        fs::remove_file(self.directory.join("occupied"))?;
        // Other concurrent watchdog forks can still hold duplicate descriptors.
        // Close alone would retain their shared flock until every copy closes.
        FileExt::unlock(&self._lock).context("unlock cleaned validator slot")?;
        Ok(())
    }
}

fn prune(root: &Path, relative: &Path, caches: &[PathBuf]) -> Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        if caches.iter().any(|cache| cache == &path) {
            continue;
        }
        if caches.iter().any(|cache| cache.starts_with(&path)) {
            ensure!(
                entry.file_type()?.is_dir(),
                "cache ancestor is not a directory"
            );
            prune(root, &path, caches)?;
        } else if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn release_unlocks_even_while_a_duplicate_descriptor_survives() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir(&source).unwrap();
        let pool = SlotPool::open(
            &directory.path().join("pool"),
            &source,
            SlotSettings {
                count: 1,
                port_base: 24000,
                ports_per_slot: 1,
                caches: vec![],
            },
        )
        .unwrap();
        let lease = pool.acquire().unwrap();
        let duplicate = lease._lock.try_clone().unwrap();
        lease.release().unwrap();
        let next = pool.acquire().expect(
            "successful release must unlock inherited/duplicated file descriptions immediately",
        );
        next.release().unwrap();
        drop(duplicate);
    }
    #[test]
    fn only_native_lock_contention_is_treated_as_an_occupied_slot() {
        assert!(lock_is_contended(&fs2::lock_contended_error()));
        assert!(lock_is_contended(&std::io::Error::from(
            std::io::ErrorKind::WouldBlock
        )));
        assert!(!lock_is_contended(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
        assert!(!lock_is_contended(&std::io::Error::from_raw_os_error(
            12345
        )));
    }
}
