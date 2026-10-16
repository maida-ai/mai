//! Read-only import qualification. Every retained reachable object is inspected;
//! callers must revalidate a staged import before publishing it to close source races.
use anyhow::{Context, Result, bail, ensure};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct ValidatedSource {
    pub path: PathBuf,
    pub head: Option<String>,
}

/// Convert canonical Windows paths at the Git argument boundary only. Filesystem
/// and IPC identities retain their original canonical representation. Ordinary,
/// relative and device paths are unchanged; this does not qualify network shares.
pub fn external_git_path(path: &Path) -> std::ffi::OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        std::ffi::OsString::from_wide(&windows_external_units(
            &path.as_os_str().encode_wide().collect::<Vec<_>>(),
        ))
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().to_owned()
    }
}

#[cfg(any(windows, test))]
fn windows_external_units(path: &[u16]) -> Vec<u16> {
    // Work in UTF-16 units: valid Windows paths can contain unpaired surrogates.
    const VERBATIM: &[u16] = &[92, 92, 63, 92];
    const UNC: &[u16] = &[92, 92, 63, 92, 85, 78, 67, 92];
    if path.starts_with(VERBATIM)
        && path.len() >= 7
        && ((65..=90).contains(&path[4]) || (97..=122).contains(&path[4]))
        && path[5] == 58
        && path[6] == 92
    {
        path[4..].to_vec()
    } else if path.starts_with(UNC) {
        [vec![92, 92], path[8..].to_vec()].concat()
    } else {
        path.to_vec()
    }
}

fn git(path: &Path) -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.arg("-C")
        .arg(external_git_path(path))
        .args([
            "-c",
            "protocol.allow=never",
            "-c",
            "core.hooksPath=",
            "-c",
            "core.fsmonitor=false",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}
fn output(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = git(path)
        .args(args)
        .output()
        .context("execute Git plumbing")?;
    // Do not echo repository-controlled config, paths, or object contents.
    ensure!(
        out.status.success(),
        "Git {} failed ({})",
        args[0],
        out.status
    );
    Ok(out.stdout)
}
fn version_supported(version: &str) -> bool {
    let Some(number) = version.trim().strip_prefix("git version ") else {
        return false;
    };
    let mut parts = number.split('.');
    let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
        return false;
    };
    match (major.parse::<u32>(), minor.parse::<u32>()) {
        (Ok(major), Ok(minor)) => (major, minor) >= (2, 40),
        _ => false,
    }
}
pub fn validate_git_version() -> Result<()> {
    let bytes = output(Path::new("."), &["--version"])?;
    ensure!(
        version_supported(std::str::from_utf8(&bytes)?),
        "Git >= 2.40 is required"
    );
    Ok(())
}

/// Qualify an existing worktree root or bare repository without checking out files,
/// executing hooks/filters, fetching objects, or changing refs/index/worktree data.
/// Shallow, grafted and partial histories cannot establish complete import scope.
pub fn validate_source(path: &Path) -> Result<ValidatedSource> {
    validate_git_version()?;
    let path = path
        .canonicalize()
        .context("source repository does not exist")?;
    ensure!(
        path.join(".git").exists()
            || (path.join("HEAD").is_file() && path.join("objects").is_dir()),
        "source must be a repository root or bare repository"
    );
    ensure!(
        output(&path, &["rev-parse", "--show-object-format"])? == b"sha1\n",
        "only SHA-1 repositories are supported"
    );
    ensure!(
        output(&path, &["rev-parse", "--is-shallow-repository"])? == b"false\n",
        "shallow repositories are unsupported; complete history is required"
    );
    let grafts = output(&path, &["rev-parse", "--git-path", "info/grafts"])?;
    let graft_path = path_from_git(&grafts)?;
    let graft_path = if graft_path.is_absolute() {
        graft_path
    } else {
        path.join(graft_path)
    };
    ensure!(!graft_path.exists(), "grafted histories are unsupported");
    let partial = git(&path)
        .args([
            "config",
            "--get-regexp",
            "^(extensions\\.partialclone|remote\\..*\\.promisor)$",
        ])
        .output()?;
    ensure!(
        partial.status.code() == Some(1),
        "partial clone configuration is unsupported"
    );
    let head_result = git(&path)
        .args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .output()?;
    let head = if head_result.status.success() {
        Some(std::str::from_utf8(&head_result.stdout)?.trim().to_owned())
    } else {
        let symbolic = output(&path, &["symbolic-ref", "-q", "HEAD"])
            .context("HEAD must identify a commit or an unborn branch")?;
        let reference = std::str::from_utf8(&symbolic)?.trim();
        let exists = git(&path)
            .args(["show-ref", "--verify", "--quiet", reference])
            .status()?;
        ensure!(
            exists.code() == Some(1),
            "HEAD does not identify a valid commit"
        );
        None
    };
    let mut args = vec!["rev-list", "--objects", "--no-object-names", "--all"];
    if head.is_some() {
        args.push("HEAD");
    }
    let objects = output(&path, &args)?;
    let mut attrs = HashSet::new();
    // Two streaming passes avoid retaining blob contents or trusting traversal order.
    // This intentionally trades import time for complete historical qualification.
    visit_objects(&path, &objects, |_oid, kind, data| {
        if kind == "tree" {
            let mut tree = Vec::new();
            data.read_to_end(&mut tree)?;
            let mut remainder = tree.as_slice();
            while !remainder.is_empty() {
                let nul = remainder
                    .iter()
                    .position(|b| *b == 0)
                    .context("malformed tree")?;
                let entry = &remainder[..nul];
                let space = entry
                    .iter()
                    .position(|b| *b == b' ')
                    .context("malformed tree entry")?;
                ensure!(
                    &entry[..space] != b"160000",
                    "submodule gitlinks are unsupported"
                );
                ensure!(remainder.len() >= nul + 21, "malformed SHA-1 tree entry");
                if &entry[space + 1..] == b".gitattributes" {
                    attrs.insert(
                        remainder[nul + 1..nul + 21]
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>(),
                    );
                }
                remainder = &remainder[nul + 21..];
            }
        }
        Ok(())
    })?;
    visit_objects(&path, &objects, |oid, kind, data| {
        if kind != "blob" {
            return Ok(());
        }
        if attrs.contains(oid) {
            let mut reader = BufReader::new(data);
            let mut line = Vec::new();
            while reader.read_until(b'\n', &mut line)? > 0 {
                let tokens: Vec<_> = line
                    .split(|b| b.is_ascii_whitespace())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !tokens.first().is_some_and(|s| s.starts_with(b"#")) {
                    ensure!(
                        !tokens.contains(&b"filter=lfs".as_slice()),
                        "Git LFS attributes are unsupported"
                    );
                }
                ensure!(
                    !line.starts_with(b"version https://git-lfs.github.com/spec/v1"),
                    "Git LFS pointers are unsupported"
                );
                line.clear();
            }
        } else {
            let mut prefix = [0; 64];
            let mut count = 0;
            while count < prefix.len() {
                let n = data.read(&mut prefix[count..])?;
                if n == 0 {
                    break;
                }
                count += n;
            }
            ensure!(
                !prefix[..count].starts_with(b"version https://git-lfs.github.com/spec/v1"),
                "Git LFS pointers are unsupported"
            );
        }
        Ok(())
    })?;
    Ok(ValidatedSource { path, head })
}
fn path_from_git(bytes: &[u8]) -> Result<PathBuf> {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(bytes.to_vec()).into())
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(std::str::from_utf8(bytes)?))
    }
}
fn visit_objects(
    path: &Path,
    objects: &[u8],
    mut visit: impl FnMut(&str, &str, &mut dyn Read) -> Result<()>,
) -> Result<()> {
    let mut input = tempfile::tempfile()?;
    input.write_all(objects)?;
    input.rewind()?;
    let mut child = git(path)
        .args(["cat-file", "--batch"])
        .stdin(input)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut reader = BufReader::new(child.stdout.take().context("missing Git stdout")?);
    let result = (|| -> Result<()> {
        let mut header = String::new();
        while reader.read_line(&mut header)? > 0 {
            let parts: Vec<_> = header.split_whitespace().collect();
            ensure!(parts.len() == 3, "missing or malformed retained Git object");
            let size: u64 = parts[2].parse()?;
            let mut data = reader.by_ref().take(size);
            visit(parts[0], parts[1], &mut data)?;
            std::io::copy(&mut data, &mut std::io::sink())?;
            ensure!(data.limit() == 0, "truncated Git object");
            let mut newline = [0];
            reader.read_exact(&mut newline)?;
            ensure!(newline == *b"\n", "malformed Git object framing");
            header.clear();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    result?;
    if !status.success() {
        bail!("cannot read complete retained Git object history");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_windows_paths_preserve_native_units() {
        let wide = |s: &str| s.encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            windows_external_units(&wide(r"\\?\C:\space here\repo")),
            wide(r"C:\space here\repo")
        );
        assert_eq!(
            windows_external_units(&wide(r"\\?\UNC\server\share\repo")),
            wide(r"\\server\share\repo")
        );
        for path in [
            r"relative\repo",
            r"C:\repo",
            r"\\server\share",
            r"\\.\device",
            r"\\?\Volume{abc}\",
        ] {
            assert_eq!(windows_external_units(&wide(path)), wide(path));
        }
        let mut unpaired = wide(r"\\?\C:\repo\");
        unpaired.push(0xd800);
        assert_eq!(windows_external_units(&unpaired), unpaired[4..]);
    }
    #[cfg(unix)]
    #[test]
    fn external_unix_paths_preserve_raw_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(
            b"/tmp/raw-\xff\\name".to_vec(),
        ));
        assert_eq!(external_git_path(&path), path.as_os_str());
    }
    #[cfg(windows)]
    #[test]
    fn external_native_windows_arguments_use_ordinary_prefixes() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let original = std::ffi::OsString::from_wide(&[92, 92, 63, 92, 67, 58, 92, 0xd800]);
        assert_eq!(
            external_git_path(Path::new(&original))
                .encode_wide()
                .collect::<Vec<_>>(),
            vec![67, 58, 92, 0xd800]
        );
    }
    #[test]
    fn minimum_version_and_vendor_suffixes() {
        for v in [
            "git version 2.40.0",
            "git version 2.55.0.windows.5",
            "git version 2.43.0 (Apple Git-152)",
            "git version 3.0.0",
        ] {
            assert!(version_supported(v), "{v}");
        }
        for v in [
            "git version 2.39.9",
            "git version 1.99.0",
            "git version nope",
            "2.40",
            "git version 2.40garbage",
        ] {
            assert!(!version_supported(v), "{v}");
        }
    }
}
