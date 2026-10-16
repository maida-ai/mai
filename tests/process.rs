use mai::process::{self, CancellationToken, ProcessOutcome, RunSpec};
use std::{collections::BTreeMap, path::Path, time::Duration};
fn spec(directory: &Path, script: &str) -> RunSpec {
    #[cfg(unix)]
    let (program, args) = ("sh", vec!["-c".into(), script.into()]);
    #[cfg(windows)]
    let (program, args) = (
        "cmd.exe",
        vec!["/D".into(), "/S".into(), "/C".into(), script.into()],
    );
    RunSpec {
        program: program.into(),
        args,
        cwd: directory.to_owned(),
        env: BTreeMap::new(),
        timeout: Duration::from_secs(5),
        output_tail_bytes: 32,
    }
}
#[test]
fn success_failure_missing_command_and_pre_cancel() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = spec(directory.path(), "echo hello");
    let result = process::run(&command, &CancellationToken::default()).unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Success);
    assert!(result.stdout_tail.contains("hello"));
    command = spec(directory.path(), "exit 7");
    let result = process::run(&command, &CancellationToken::default()).unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Failure);
    assert_eq!(result.exit_code, Some(7));
    command.program = "mai-no-such-command-xyz".into();
    assert!(process::run(&command, &CancellationToken::default()).is_err());
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        process::run(&command, &token).unwrap().outcome,
        ProcessOutcome::Cancelled
    );
}
#[cfg(unix)]
#[test]
fn floods_are_drained_with_bounded_tails() {
    let directory = tempfile::tempdir().unwrap();
    let command = spec(
        directory.path(),
        "i=0; while [ $i -lt 20000 ]; do printf abcdefgh; printf ABCDEFGH >&2; i=$((i+1)); done; printf end; printf END >&2",
    );
    let result = process::run(&command, &CancellationToken::default()).unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Success);
    assert_eq!(result.stdout_tail.len(), 32);
    assert!(result.stdout_tail.ends_with("end"));
    assert_eq!(result.stderr_tail.len(), 32);
    assert!(result.stderr_tail.ends_with("END"));
}
#[cfg(unix)]
#[test]
fn timeout_and_cancellation_stop_nested_processes() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = spec(directory.path(), "sh -c 'sleep 1; touch escaped' & wait");
    command.timeout = Duration::from_millis(60);
    assert_eq!(
        process::run(&command, &CancellationToken::default())
            .unwrap()
            .outcome,
        ProcessOutcome::Timeout
    );
    let token = CancellationToken::default();
    let copy = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        copy.cancel();
    });
    command.timeout = Duration::from_secs(5);
    assert_eq!(
        process::run(&command, &token).unwrap().outcome,
        ProcessOutcome::Cancelled
    );
    std::thread::sleep(Duration::from_millis(1100));
    assert!(!directory.path().join("escaped").exists());
}
#[cfg(unix)]
#[test]
fn separate_process_owner_helper() {
    let Some(directory) = std::env::var_os("MAI_PROCESS_OWNER_TEST") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let command = spec(
        &directory,
        "echo started > started; sh -c 'sleep 1; touch escaped' & wait",
    );
    process::run(&command, &CancellationToken::default()).unwrap();
}
#[cfg(unix)]
#[test]
fn abrupt_owner_and_owner_group_death_still_kill_descendants() {
    use std::os::unix::process::CommandExt;
    for group in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "separate_process_owner_helper", "--nocapture"])
            .env("MAI_PROCESS_OWNER_TEST", directory.path());
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        let start = std::time::Instant::now();
        while !directory.path().join("started").exists() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
        unsafe {
            libc::kill(
                if group {
                    -(child.id() as i32)
                } else {
                    child.id() as i32
                },
                libc::SIGKILL,
            );
        }
        child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        assert!(
            !directory.path().join("escaped").exists(),
            "owner-group kill escaped={group}"
        );
    }
}

#[test]
fn native_tree_helper() {
    let Some(role) = std::env::var_os("MAI_NATIVE_TREE_ROLE") else {
        return;
    };
    let directory = std::path::PathBuf::from(std::env::var_os("MAI_NATIVE_TREE_DIR").unwrap());
    if role == "leaf" {
        std::fs::write(directory.join("leaf-started"), "ready").unwrap();
        std::thread::sleep(Duration::from_secs(2));
        std::fs::write(directory.join("escaped"), "alive").unwrap();
    } else {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "native_tree_helper"])
            .env("MAI_NATIVE_TREE_ROLE", "leaf")
            .spawn()
            .unwrap();
        child.wait().unwrap();
    }
}
#[test]
fn native_nested_timeout_and_cancellation_are_bounded() {
    for cancel in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let token = CancellationToken::default();
        if cancel {
            let copy = token.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(150));
                copy.cancel();
            });
        }
        let result = process::run(
            &RunSpec {
                program: std::env::current_exe().unwrap().into_os_string(),
                args: vec!["--exact".into(), "native_tree_helper".into()],
                cwd: directory.path().to_owned(),
                env: [
                    ("MAI_NATIVE_TREE_ROLE".into(), "parent".into()),
                    (
                        "MAI_NATIVE_TREE_DIR".into(),
                        directory.path().as_os_str().to_owned(),
                    ),
                ]
                .into(),
                timeout: if cancel {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(150)
                },
                output_tail_bytes: 256,
            },
            &token,
        )
        .unwrap();
        assert_eq!(
            result.outcome,
            if cancel {
                ProcessOutcome::Cancelled
            } else {
                ProcessOutcome::Timeout
            }
        );
        assert!(result.elapsed_ms < 2000);
        std::thread::sleep(Duration::from_millis(2100));
        assert!(!directory.path().join("escaped").exists());
    }
}
#[test]
fn native_owner_helper() {
    let Some(directory) = std::env::var_os("MAI_NATIVE_OWNER_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let mut command = spec(&directory, "exit 0");
    command.program = std::env::current_exe().unwrap().into_os_string();
    command.args = vec!["--exact".into(), "native_tree_helper".into()];
    command.env = [
        ("MAI_NATIVE_TREE_ROLE".into(), "parent".into()),
        (
            "MAI_NATIVE_TREE_DIR".into(),
            directory.as_os_str().to_owned(),
        ),
    ]
    .into();
    process::run(&command, &CancellationToken::default()).unwrap();
}
#[test]
fn native_owner_death_closes_descendant_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let mut owner = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native_owner_helper"])
        .env("MAI_NATIVE_OWNER_DIR", directory.path())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !directory.path().join("leaf-started").exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    owner.kill().unwrap();
    owner.wait().unwrap();
    std::thread::sleep(Duration::from_millis(2100));
    assert!(!directory.path().join("escaped").exists());
}
