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
    #[serde(skip)]
    pub supervised_recovery: bool,
}
#[derive(Clone)]
pub struct SlotPool {
    root: PathBuf,
    source: PathBuf,
    settings: SlotSettings,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
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
    claim_id: String,
    claim: SlotClaim,
    last_run_outcome: Option<ProcessOutcome>,
    receipt_published: bool,
    claim_released: bool,
    healthy: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SlotClaim {
    version: u32,
    pool: NativePath,
    slot: usize,
    claim_id: String,
    boot_id: String,
    recovery_contract: String,
    binding: Option<crate::policy::Binding>,
    #[serde(default)]
    temporary: Option<NativePath>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CleanupReceipt {
    version: u32,
    pool: NativePath,
    slot: usize,
    claim_id: String,
    worker_id: String,
    boot_id: String,
    quiescence: String,
    recovery_contract: String,
    binding: Option<crate::policy::Binding>,
    temporary: Option<NativePath>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryJournal {
    version: u32,
    claim: SlotClaim,
    receipt: CleanupReceipt,
    phase: String,
    #[serde(default)]
    temporary_quarantine: Option<NativePath>,
}

fn boot_id() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
        let id = id.trim();
        ensure!(!id.is_empty(), "host boot identity is unavailable");
        Ok(id.to_owned())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok("unsupported".into())
    }
}

fn new_claim_id() -> Result<String> {
    let nonce = tempfile::Builder::new()
        .prefix("mai-claim-")
        .rand_bytes(32)
        .tempfile()?;
    let name = nonce
        .path()
        .file_name()
        .context("temporary claim nonce has no filename")?
        .to_string_lossy()
        .into_owned();
    drop(nonce);
    Ok(name)
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
fn git(
    cwd: &Path,
    arguments: &[&std::ffi::OsStr],
    cancellation: &CancellationToken,
    last_outcome: &mut Option<ProcessOutcome>,
) -> Result<String> {
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
        cancellation,
    )?;
    *last_outcome = Some(result.outcome.clone());
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
                record.source.as_path() == source
                    && record.settings.count == settings.count
                    && record.settings.port_base == settings.port_base
                    && record.settings.ports_per_slot == settings.ports_per_slot
                    && record.settings.caches == settings.caches,
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
        self.acquire_bound(None)
    }
    pub fn acquire_bound(&self, binding: Option<&crate::policy::Binding>) -> Result<SlotLease> {
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
            let claim_id = new_claim_id()?;
            let mut claim_record = SlotClaim {
                version: 1,
                pool: NativePath::from_path(&self.root)?,
                slot: index,
                claim_id: claim_id.clone(),
                boot_id: boot_id()?,
                recovery_contract: if self.settings.supervised_recovery {
                    "supervised-local-v1".into()
                } else {
                    String::new()
                },
                binding: binding.cloned(),
                temporary: None,
            };
            serde_json::to_writer(&marker, &claim_record)?;
            marker.sync_all()?;
            sync_directory(&directory)?;
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
            claim_record.temporary = Some(NativePath::from_path(&temporary)?);
            let mut marker = OpenOptions::new().write(true).truncate(true).open(&claim)?;
            serde_json::to_writer(&mut marker, &claim_record)?;
            marker.sync_all()?;
            sync_directory(&directory)?;
            return Ok(SlotLease {
                pool: self.clone(),
                index,
                directory,
                clone,
                temporary,
                _lock: lock,
                claim_id,
                claim: claim_record,
                last_run_outcome: None,
                receipt_published: false,
                claim_released: false,
                healthy: true,
            });
        }
        bail!(
            "no validator slot available ({}); occupied/interrupted slots require trusted cleanup before reuse",
            unavailable.join("; ")
        )
    }

    /// Reclaim one interrupted slot only from a claim-bound receipt written by
    /// the supervised worker after its native process runner confirmed cleanup.
    /// The caller must already hold the authority engine lock.
    pub fn inspect(&self, index: usize) -> Result<SlotInspection> {
        ensure!(
            index < self.settings.count,
            "validator slot index out of range"
        );
        let directory = self.root.join(format!("slot-{index}"));
        ensure!(
            !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
            "slot directory cannot be a symlink"
        );
        let claim_path = directory.join("occupied");
        if !claim_path.exists() {
            return Ok(SlotInspection {
                schema_version: 1,
                slot: index,
                occupied: false,
                claim_id: None,
                candidate: None,
                cleanup_receipt: false,
            });
        }
        ensure!(
            !fs::symlink_metadata(&claim_path)?.file_type().is_symlink(),
            "occupied claim cannot be a symlink"
        );
        let claim: SlotClaim = serde_json::from_slice(&fs::read(&claim_path)?)
            .context("occupied claim is legacy or malformed; recovery is unavailable")?;
        Ok(SlotInspection {
            schema_version: 1,
            slot: index,
            occupied: true,
            claim_id: Some(claim.claim_id),
            candidate: claim.binding.map(|binding| binding.candidate),
            cleanup_receipt: directory.join("cleanup-receipt.json").is_file(),
        })
    }

    pub fn recover(&self, index: usize, expected_claim_id: &str) -> Result<PathBuf> {
        ensure!(
            index < self.settings.count,
            "validator slot index out of range"
        );
        ensure!(
            expected_claim_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "claim ID has invalid path characters"
        );
        ensure!(
            self.settings.supervised_recovery,
            "trusted policy does not enable supervised-local recovery"
        );
        let directory = self.root.join(format!("slot-{index}"));
        ensure!(directory.is_dir(), "validator slot directory is missing");
        ensure!(
            !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
            "slot directory cannot be a symlink"
        );
        for name in [
            "lock",
            "occupied",
            "cleanup-receipt.json",
            "recovery-journal.json",
        ] {
            let path = directory.join(name);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "recovery refuses symlink {}",
                    path.display()
                );
            }
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("lock"))?;
        match lock.try_lock_exclusive() {
            Ok(()) => {}
            Err(error) if lock_is_contended(&error) => {
                bail!("validator slot {index} is still owned by a worker")
            }
            Err(error) => return Err(error).context("lock validator slot for recovery"),
        }
        let occupied = directory.join("occupied");
        if !occupied.exists() {
            let quarantine = directory.join(format!("quarantine-{expected_claim_id}"));
            ensure!(
                !fs::symlink_metadata(&quarantine)?.file_type().is_symlink(),
                "quarantine directory cannot be a symlink"
            );
            let journal: RecoveryJournal =
                serde_json::from_slice(&fs::read(quarantine.join("recovery-journal.json"))?)
                    .context("completed recovery record is missing")?;
            ensure!(
                journal.claim.claim_id == expected_claim_id,
                "completed recovery record has a different claim ID"
            );
            validate_journal(&journal, &journal.claim, &self.root, index)?;
            ensure!(
                journal.phase == "artifacts-quarantined",
                "recovery is not complete"
            );
            ensure!(
                !directory.join("cleanup-receipt.json").exists(),
                "completed recovery still has a cleanup receipt"
            );
            return Ok(quarantine);
        }
        let claim: SlotClaim = serde_json::from_slice(&fs::read(&occupied)?)
            .context("occupied claim is not a supported recoverable claim")?;
        ensure!(claim.version == 1, "unsupported validator claim version");
        ensure!(
            claim.pool.as_path() == self.root,
            "claim belongs to another slot pool"
        );
        ensure!(claim.slot == index, "claim belongs to another slot");
        ensure!(
            claim.recovery_contract == "supervised-local-v1",
            "claim lacks recovery opt-in"
        );
        ensure!(
            claim.boot_id != "unsupported" && claim.boot_id == boot_id()?,
            "claim host boot identity does not match"
        );
        ensure!(
            claim.binding.is_some(),
            "claim does not retain its original trusted binding"
        );
        ensure!(
            claim.claim_id == expected_claim_id,
            "claim ID changed; inspect the slot again before recovery"
        );
        ensure!(
            claim
                .claim_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "claim ID has invalid path characters"
        );

        let journal_path = directory.join("recovery-journal.json");
        let quarantine = directory.join(format!("quarantine-{}", claim.claim_id));
        let archived_journal = quarantine.join("recovery-journal.json");
        if let Ok(meta) = fs::symlink_metadata(&quarantine) {
            ensure!(
                meta.file_type().is_dir(),
                "quarantine path is not an ordinary directory"
            );
        }
        if archived_journal.exists() && !journal_path.exists() {
            fs::rename(&archived_journal, &journal_path)?;
        }
        let mut journal = if journal_path.exists() {
            let journal: RecoveryJournal = serde_json::from_slice(&fs::read(&journal_path)?)
                .context("invalid recovery journal")?;
            validate_journal(&journal, &claim, &self.root, index)?;
            if directory.join("cleanup-receipt.json").exists() {
                let disk_receipt: CleanupReceipt =
                    serde_json::from_slice(&fs::read(directory.join("cleanup-receipt.json"))?)?;
                ensure!(
                    disk_receipt == journal.receipt,
                    "cleanup receipt changed after journal creation"
                );
            }
            journal
        } else if archived_journal.exists() {
            let journal: RecoveryJournal = serde_json::from_slice(&fs::read(&archived_journal)?)
                .context("invalid archived recovery journal")?;
            validate_journal(&journal, &claim, &self.root, index)?;
            journal
        } else {
            let receipt: CleanupReceipt =
                serde_json::from_slice(&fs::read(directory.join("cleanup-receipt.json"))?)
                    .context("missing or invalid verified cleanup receipt")?;
            ensure!(receipt.version == 1, "unsupported cleanup receipt version");
            ensure!(
                receipt.pool.as_path() == self.root && receipt.slot == index,
                "cleanup receipt belongs to another slot pool"
            );
            validate_receipt(&receipt, &claim, &self.root, index)?;
            ensure!(
                receipt.boot_id == claim.boot_id && receipt.boot_id == boot_id()?,
                "cleanup receipt host boot identity does not match"
            );
            ensure!(
                receipt.quiescence == "native-processes-quiescent-v1",
                "cleanup receipt lacks verified quiescence"
            );
            let journal = RecoveryJournal {
                version: 1,
                claim: serde_json::from_slice(&fs::read(directory.join("occupied"))?)?,
                receipt,
                phase: "prepared".into(),
                temporary_quarantine: None,
            };
            write_json_sync(&journal_path, &journal)?;
            #[cfg(all(test, target_os = "linux"))]
            recovery_crash_tests::checkpoint("journal-written");
            journal
        };
        if let Some(record_path) = [
            directory.join("temporary.json"),
            quarantine.join("temporary.json"),
        ]
        .into_iter()
        .find(|path| path.exists())
        {
            let temp_record: NativePath = serde_json::from_slice(&fs::read(&record_path)?)?;
            ensure!(
                claim.temporary.as_ref() == Some(&temp_record),
                "temporary workspace record disagrees with original claim"
            );
        }
        if !quarantine.exists() {
            private_directory(&quarantine)?;
        }
        ensure!(
            !fs::symlink_metadata(&quarantine)?.file_type().is_symlink(),
            "quarantine directory cannot be a symlink"
        );
        for name in ["candidate", "temporary.json", "cache-staging"] {
            let source = directory.join(name);
            if source.exists() {
                let target = quarantine.join(name);
                ensure!(
                    !target.exists(),
                    "both source and quarantine artifact exist for {name}"
                );
                fs::rename(&source, &target)?;
            }
        }
        let temp_record = quarantine.join("temporary.json");
        if temp_record.exists() {
            let temp_path_record: NativePath = serde_json::from_slice(&fs::read(&temp_record)?)?;
            ensure!(
                claim.temporary.as_ref() == Some(&temp_path_record),
                "temporary workspace record disagrees with original claim"
            );
            let temp_path = temp_path_record.as_path();
            let metadata = if temp_path.exists() {
                Some(fs::symlink_metadata(temp_path)?)
            } else {
                None
            };
            if let Some(metadata) = &metadata {
                ensure!(
                    metadata.file_type().is_dir(),
                    "temporary workspace is not an ordinary directory"
                );
            }
            let parent = temp_path
                .parent()
                .context("temporary workspace has no parent")?;
            ensure!(
                temp_path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("mai-validator-")),
                "temporary workspace name is not service-generated"
            );
            ensure!(
                parent == std::env::temp_dir(),
                "temporary workspace is outside system temp root"
            );
            let expected_destination = parent.join(format!("mai-quarantine-{}", claim.claim_id));
            let destination = if let Some(destination) = &journal.temporary_quarantine {
                ensure!(
                    destination.as_path() == expected_destination,
                    "journaled temporary quarantine does not match original claim"
                );
                destination.as_path().to_path_buf()
            } else {
                let destination = expected_destination;
                journal.temporary_quarantine = Some(NativePath::from_path(&destination)?);
                journal.phase = "temporary-quarantine-planned".into();
                write_json_sync(&journal_path, &journal)?;
                destination
            };
            ensure!(
                destination.parent() == Some(parent),
                "journaled temporary quarantine is outside source filesystem"
            );
            let source_exists = temp_path.exists();
            let destination_exists = destination.exists();
            match (source_exists, destination_exists) {
                (true, false) => fs::rename(temp_path, &destination)?,
                (false, true) => {}
                (true, true) => bail!("both temporary workspace and its quarantine exist"),
                (false, false) => bail!("temporary workspace and its quarantine are both missing"),
            }
            let evidence = quarantine.join("external-temporary.json");
            if evidence.exists() {
                let recorded: NativePath = serde_json::from_slice(&fs::read(&evidence)?)?;
                ensure!(
                    recorded.as_path() == destination,
                    "external temporary quarantine evidence disagrees with journal"
                );
            } else {
                write_json_sync(&evidence, &NativePath::from_path(&destination)?)?;
            }
        }
        journal.phase = "artifacts-quarantined".into();
        write_json_sync(&journal_path, &journal)?;
        #[cfg(all(test, target_os = "linux"))]
        recovery_crash_tests::checkpoint("artifacts-quarantined");
        match fs::remove_file(directory.join("cleanup-receipt.json")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if journal_path.exists() {
            fs::rename(&journal_path, &archived_journal)?;
        }
        fs::remove_file(directory.join("occupied"))?;
        FileExt::unlock(&lock)?;
        sync_directory(&directory)?;
        #[cfg(all(test, target_os = "linux"))]
        recovery_crash_tests::checkpoint("claim-cleared");
        Ok(quarantine)
    }
}

fn validate_journal(
    journal: &RecoveryJournal,
    claim: &SlotClaim,
    pool: &Path,
    slot: usize,
) -> Result<()> {
    ensure!(journal.version == 1, "unsupported recovery journal version");
    ensure!(
        matches!(
            journal.phase.as_str(),
            "prepared" | "temporary-quarantine-planned" | "artifacts-quarantined"
        ),
        "invalid recovery journal phase"
    );
    ensure!(
        journal.claim.version == claim.version
            && journal.claim.pool == claim.pool
            && journal.claim.slot == claim.slot
            && journal.claim.claim_id == claim.claim_id
            && journal.claim.boot_id == claim.boot_id
            && journal.claim.recovery_contract == claim.recovery_contract
            && journal.claim.binding == claim.binding
            && journal.claim.temporary == claim.temporary,
        "recovery journal claim does not match occupied claim"
    );
    let receipt = &journal.receipt;
    ensure!(receipt.version == 1, "unsupported journal receipt version");
    ensure!(
        receipt.pool.as_path() == pool && receipt.slot == slot,
        "journal receipt belongs to another pool or slot"
    );
    ensure!(
        receipt.claim_id == claim.claim_id && receipt.boot_id == claim.boot_id,
        "journal receipt does not match occupied claim"
    );
    ensure!(
        receipt.recovery_contract == claim.recovery_contract
            && receipt.binding == claim.binding
            && receipt.temporary == claim.temporary,
        "journal receipt does not bind the original claim"
    );
    ensure!(
        receipt.boot_id == boot_id()?,
        "journal host boot identity does not match"
    );
    ensure!(
        !receipt.worker_id.is_empty(),
        "journal receipt lacks worker identity"
    );
    ensure!(
        receipt.quiescence == "native-processes-quiescent-v1",
        "journal receipt lacks verified quiescence"
    );
    Ok(())
}

fn validate_receipt(
    receipt: &CleanupReceipt,
    claim: &SlotClaim,
    pool: &Path,
    slot: usize,
) -> Result<()> {
    ensure!(receipt.version == 1, "unsupported cleanup receipt version");
    ensure!(
        receipt.pool.as_path() == pool && receipt.slot == slot,
        "cleanup receipt belongs to another pool or slot"
    );
    ensure!(
        receipt.claim_id == claim.claim_id,
        "cleanup receipt belongs to another claim"
    );
    ensure!(
        receipt.recovery_contract == claim.recovery_contract,
        "cleanup receipt contract differs from original claim"
    );
    ensure!(
        receipt.binding == claim.binding,
        "cleanup receipt binding differs from original claim"
    );
    ensure!(
        receipt.temporary == claim.temporary,
        "cleanup receipt temporary path differs from original claim"
    );
    ensure!(
        receipt.boot_id == claim.boot_id && receipt.boot_id == boot_id()?,
        "cleanup receipt host boot identity does not match"
    );
    ensure!(
        !receipt.worker_id.is_empty(),
        "cleanup receipt lacks worker identity"
    );
    ensure!(
        receipt.quiescence == "native-processes-quiescent-v1",
        "cleanup receipt lacks verified quiescence"
    );
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SlotInspection {
    pub schema_version: u32,
    pub slot: usize,
    pub occupied: bool,
    pub claim_id: Option<String>,
    pub candidate: Option<String>,
    pub cleanup_receipt: bool,
}

fn write_json_sync(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    serde_json::to_writer(&file, value)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    sync_directory(path.parent().context("journal has no parent")?)
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
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
        self.materialize_with_cancel(candidate, &CancellationToken::default())
    }
    pub fn materialize_with_cancel(
        &mut self,
        candidate: &str,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        self.healthy = false;
        self.last_run_outcome = None;
        let external_source = external_git_path(&self.pool.source);
        let external_clone = external_git_path(&self.clone);
        ensure!(
            (candidate.len() == 40 || candidate.len() == 64)
                && candidate.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "candidate must be an exact full commit identity"
        );
        self.clean()?;
        ensure!(
            !cancellation.is_cancelled(),
            "validator worker lifeline closed before materialization"
        );
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
            cancellation,
            &mut self.last_run_outcome,
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
            cancellation,
            &mut self.last_run_outcome,
        )?;
        let resolved = git(
            &self.clone,
            &[
                "rev-parse".as_ref(),
                "--verify".as_ref(),
                "FETCH_HEAD^{commit}".as_ref(),
            ],
            cancellation,
            &mut self.last_run_outcome,
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
                cancellation,
                &mut self.last_run_outcome,
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
            cancellation,
            &mut self.last_run_outcome,
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
        self.last_run_outcome = None;
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
        self.last_run_outcome = Some(result.outcome.clone());
        Ok(result)
    }
    /// Retain an occupied claim after an engine-lifeline cancellation. The
    /// caller may invoke this only after `process::run` returned Cancelled,
    /// which means native cleanup and reader quiescence were confirmed.
    pub fn retain_after_verified_cancellation(mut self, worker_id: String) -> Result<()> {
        ensure!(
            self.pool.settings.supervised_recovery,
            "trusted policy does not enable supervised-local recovery"
        );
        self.publish_verified_cleanup(worker_id)?;
        self.healthy = false;
        FileExt::unlock(&self._lock).context("unlock receipt-published validator slot")?;
        Ok(())
    }
    fn publish_verified_cleanup(&mut self, worker_id: String) -> Result<()> {
        ensure!(
            self.pool.settings.supervised_recovery,
            "trusted policy does not enable supervised-local recovery"
        );
        ensure!(
            self.last_run_outcome == Some(ProcessOutcome::Cancelled),
            "cleanup receipt requires a verified cancelled process run"
        );
        let claim: SlotClaim = serde_json::from_slice(&fs::read(self.directory.join("occupied"))?)?;
        ensure!(
            claim == self.claim,
            "occupied claim changed during worker run"
        );
        let receipt = CleanupReceipt {
            version: 1,
            pool: NativePath::from_path(&self.pool.root)?,
            slot: self.index,
            claim_id: self.claim_id.clone(),
            worker_id,
            boot_id: boot_id()?,
            quiescence: "native-processes-quiescent-v1".into(),
            recovery_contract: claim.recovery_contract.clone(),
            binding: claim.binding.clone(),
            temporary: claim.temporary.clone(),
        };
        write_json_sync(&self.directory.join("cleanup-receipt.json"), &receipt)?;
        self.receipt_published = true;
        Ok(())
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
        self.claim_released = true;
        // Other concurrent watchdog forks can still hold duplicate descriptors.
        // Close alone would retain their shared flock until every copy closes.
        FileExt::unlock(&self._lock).context("unlock cleaned validator slot")?;
        Ok(())
    }
}

impl Drop for SlotLease {
    fn drop(&mut self) {
        if !self.claim_released
            && !self.receipt_published
            && self.pool.settings.supervised_recovery
            && self.last_run_outcome == Some(ProcessOutcome::Cancelled)
        {
            let _ = self.publish_verified_cleanup(format!("worker-{}", std::process::id()));
        }
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
    #[test]
    fn claim_ids_use_a_32_character_random_component() {
        let mut observed = std::collections::HashSet::new();
        for _ in 0..16 {
            let id = new_claim_id().unwrap();
            let suffix = id.strip_prefix("mai-claim-").unwrap();
            assert_eq!(suffix.len(), 32);
            assert!(suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()));
            assert!(observed.insert(id));
        }
    }

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
                supervised_recovery: false,
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

#[cfg(all(test, target_os = "linux"))]
#[path = "slots_recovery_tests.rs"]
mod recovery_crash_tests;
