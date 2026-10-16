//! Disposable local capability evidence; no product receive path is enabled.
use mai::repository_scope::{external_git_path, qualify_git};
use std::{fs, path::Path, process::Command};

fn git(path: &Path, args: &[&std::ffi::OsStr]) -> std::process::Output {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .arg("-C")
        .arg(external_git_path(path))
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
        .output()
        .unwrap()
}
fn ok(path: &Path, args: &[&str]) -> String {
    let output = git(
        path,
        &args.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn hook(path: &Path, name: &str, script: &str) {
    fs::create_dir_all(path.join("hooks")).unwrap();
    let path = path.join("hooks").join(name);
    fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
#[test]
fn authority_minimum_executable_and_explicit_base_merge() {
    qualify_git(Path::new("git")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let error = qualify_git(&temp.path().join("missing-git")).unwrap_err();
    assert!(error.to_string().contains("install Git >= 2.40"));
}
// A concurrent fork can briefly inherit a fixture writer descriptor between
// write and exec. Retry only that OS condition, keeping real Git errors intact.
#[cfg(unix)]
fn retry_fixture_text_busy<T>(
    mut operation: impl FnMut() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match operation() {
            Err(error)
                if error.chain().any(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|io| io.raw_os_error() == Some(libc::ETXTBSY))
                }) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            result => return result,
        }
    }
}

#[cfg(unix)]
#[test]
fn fixture_retry_only_retries_text_busy_and_preserves_real_errors() {
    let mut attempts = 0;
    let error = retry_fixture_text_busy::<()>(|| {
        attempts += 1;
        if attempts == 1 {
            Err(
                anyhow::Error::new(std::io::Error::from_raw_os_error(libc::ETXTBSY))
                    .context("fixture temporarily held by inherited descriptor"),
            )
        } else {
            anyhow::bail!("expected Git diagnostic")
        }
    })
    .unwrap_err();
    assert_eq!(attempts, 2);
    assert_eq!(error.to_string(), "expected Git diagnostic");

    let mut attempts = 0;
    let error = retry_fixture_text_busy::<()>(|| {
        attempts += 1;
        Err(anyhow::Error::new(std::io::Error::from_raw_os_error(
            libc::EACCES,
        )))
    })
    .unwrap_err();
    assert_eq!(attempts, 1);
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(libc::EACCES)
    );
}

#[cfg(unix)]
#[test]
fn old_and_broken_git_fail_actionably() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    for (index, (script, message)) in [
        ("#!/bin/sh\necho 'git version 2.39.9'\n", "upgrade Git"),
        ("#!/bin/sh\nexit 42\n", "working Git >= 2.40"),
        ("#!/bin/sh\necho nonsense\n", "upgrade Git"),
        (
            "#!/bin/sh\ncase \"$*\" in *--version) echo 'git version 2.40.0';; *) exit 1;; esac\n",
            "working Git >= 2.40",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let executable = temp.path().join(format!("git-{index}"));
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let error = retry_fixture_text_busy(|| qualify_git(&executable)).unwrap_err();
        assert!(error.to_string().contains(message), "{error:#}");
    }
}
#[test]
fn transport_push_options_proc_rejection_and_retained_incoming_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let server = temp.path().join("authority.git");
    fs::create_dir(&server).unwrap();
    ok(&server, &["init", "--bare", "--template=", "."]);
    ok(temp.path(), &["init", "--template=", "."]);
    fs::write(temp.path().join("file"), "fixture").unwrap();
    ok(temp.path(), &["add", "file"]);
    ok(temp.path(), &["commit", "-m", "fixture"]);
    let oid = ok(temp.path(), &["rev-parse", "HEAD"]);
    ok(&server, &["config", "receive.advertisePushOptions", "true"]);
    ok(&server, &["config", "receive.procReceiveRefs", "refs/for/"]);
    // Exercise dispatch and fail closed before any registration acknowledgement.
    // Full packet-line registration semantics remain S1/receive work.
    hook(
        &server,
        "proc-receive",
        "#!/bin/sh\necho MAI_PROC_REJECTION >&2\nexit 1\n",
    );
    let push = |target: &str| {
        git(
            temp.path(),
            &[
                "push".as_ref(),
                "--push-option=mai-probe".as_ref(),
                external_git_path(&server).as_ref(),
                target.as_ref(),
            ],
        )
    };
    let output = push("HEAD:refs/for/trunk");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MAI_PROC_REJECTION"));
    assert!(
        !git(
            &server,
            &[
                "show-ref".as_ref(),
                "--verify".as_ref(),
                "refs/for/trunk".as_ref()
            ]
        )
        .status
        .success()
    );
    // Qualified fallback: receive stores incoming ref, while registration is a
    // separate step. Transport success MUST NOT imply queue admission.
    hook(
        &server,
        "pre-receive",
        "#!/bin/sh\n[ \"$GIT_PUSH_OPTION_COUNT\" = 1 ] && [ \"$GIT_PUSH_OPTION_0\" = mai-probe ] || exit 1\n",
    );
    let output = push("HEAD:refs/incoming/probe");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(ok(&server, &["rev-parse", "refs/incoming/probe"]), oid);
    ok(
        &server,
        &["config", "receive.advertisePushOptions", "false"],
    );
    let output = push("HEAD:refs/incoming/no-options");
    assert!(!output.status.success());
    assert!(
        !git(
            &server,
            &[
                "show-ref".as_ref(),
                "--verify".as_ref(),
                "refs/incoming/no-options".as_ref()
            ]
        )
        .status
        .success()
    );
}

#[cfg(unix)]
#[test]
fn init_rejects_insufficient_git_before_creating_destination() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("git");
    fs::write(&executable, "#!/bin/sh\necho 'git version 2.39.9'\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let destination = temp.path().join("project");
    let output = Command::new(env!("CARGO_BIN_EXE_mai"))
        .arg("init")
        .arg(&destination)
        .env("PATH", temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("upgrade Git"));
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn relative_git_executable_helper() {
    let Some(executable) = std::env::var_os("MAI_RELATIVE_GIT") else {
        return;
    };
    qualify_git(Path::new(&executable)).unwrap();
}
#[cfg(unix)]
#[test]
fn capability_probe_preserves_explicit_and_path_relative_git_resolution() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let real_git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(real_git.status.success());
    let real_git = Path::new(std::str::from_utf8(&real_git.stdout).unwrap().trim())
        .canonicalize()
        .unwrap();
    assert!(real_git.is_absolute());
    let path = root.path().join("bin");
    fs::create_dir(&path).unwrap();
    // Native symlink avoids rewriting an executable while other tests fork.
    std::os::unix::fs::symlink(real_git, path.join("git")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    for executable in ["./bin/git", "git"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "relative_git_executable_helper"])
            .current_dir(root.path())
            .env("MAI_RELATIVE_GIT", executable)
            .env("PATH", "bin")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{executable}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_mai"))
        .arg("init")
        .arg(root.path().join("project"))
        .current_dir(root.path())
        .env("PATH", "bin")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
