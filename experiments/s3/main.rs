//! S3 disposable warm-validator qualification. No product code or dependencies.
use std::{
    env, fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[derive(Clone, Copy)]
struct Fixture {
    name: &'static str,
    url: &'static str,
    rev: &'static str,
    input: &'static str,
    poison: &'static str,
}
const FIXTURES: [Fixture; 3] = [
    Fixture {
        name: "rust",
        url: "https://github.com/BurntSushi/ripgrep.git",
        rev: "4649aa9700619f94cf9c66876e9549d83420e16c",
        input: "Cargo.toml",
        poison: "src/main.rs",
    },
    Fixture {
        name: "python",
        url: "https://github.com/pallets/itsdangerous.git",
        rev: "096c8d42545d3b68ea21a4f890fb2b2d8979c0bd",
        input: "pyproject.toml",
        poison: "src/itsdangerous/__init__.py",
    },
    Fixture {
        name: "typescript",
        url: "https://github.com/colinhacks/zod.git",
        rev: "e30870369d5b8f31ff4d0130d4439fd997deb523",
        input: "package.json",
        poison: "src/index.ts",
    },
];
fn external_root(path: PathBuf, windows: bool) -> Result<PathBuf> {
    if windows {
        Ok(PathBuf::from(windows_drive_path(
            path.to_str().ok_or("Windows fixture root must be UTF-8")?,
        )?))
    } else {
        Ok(path)
    }
}
// Git and native tool launchers need ordinary drive paths, not canonical
// extended-length Windows paths. Network/device roots are outside this fixture.
fn windows_drive_path(path: &str) -> Result<String> {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    let bytes = path.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
    {
        return Err(
            "use an absolute local Windows drive for the fixture; UNC/device roots are unsupported"
                .into(),
        );
    }
    Ok(path.replace('\\', "/"))
}
fn native(name: &str) -> String {
    if cfg!(windows) && matches!(name, "npm" | "yarn") {
        format!("{name}.cmd")
    } else {
        name.into()
    }
}
fn command(slot: &Path, program: &str, args: &[&str]) -> Command {
    let mut c = Command::new(native(program));
    c.args(args).current_dir(slot);
    c.env("TMPDIR", slot.join("../tmp"))
        .env("TMP", slot.join("../tmp"))
        .env("TEMP", slot.join("../tmp"))
        .env("CARGO_TARGET_DIR", slot.join("../target"))
        .env("CARGO_HOME", slot.join("../cargo-home"))
        .env("UV_CACHE_DIR", slot.join("../uv-cache"))
        .env("UV_PYTHON_INSTALL_DIR", slot.join("../python"))
        .env("npm_config_cache", slot.join("../npm-cache"))
        .env("YARN_CACHE_FOLDER", slot.join("../yarn-cache"))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("MAI_SLOT", slot.parent().unwrap().file_name().unwrap())
        .env("MAI_PORT_BASE", "0")
        .env("HUSKY", "0")
        .env("CI", "true")
        .env("GIT_TERMINAL_PROMPT", "0");
    c
}
fn run(slot: &Path, log: &Path, program: &str, args: &[&str]) -> Result<(bool, f64)> {
    let mut file = fs::OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "cwd={} command={program} {args:?}", slot.display())?;
    let start = Instant::now();
    let mut cmd = command(slot, program, args);
    cmd.stdout(Stdio::from(file.try_clone()?))
        .stderr(Stdio::from(file.try_clone()?));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(300) {
            terminate_tree(child.id())?;
            child.wait()?;
            writeln!(file, "timeout=true limit_seconds=300")?;
            return Ok((false, start.elapsed().as_secs_f64()));
        }
        thread::sleep(Duration::from_millis(5));
    };
    let elapsed = start.elapsed().as_secs_f64();
    writeln!(file, "exit={status} elapsed_s={elapsed:.9}")?;
    Ok((status.success(), elapsed))
}
fn must(slot: &Path, log: &Path, program: &str, args: &[&str]) -> Result<f64> {
    let (ok, time) = run(slot, log, program, args)?;
    if !ok {
        return Err(format!("{program} failed; {}", log.display()).into());
    }
    Ok(time)
}
fn python(slot: &Path) -> String {
    slot.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    })
    .to_string_lossy()
    .into_owned()
}
fn yarn(root: &Path) -> String {
    root.join("tools/node_modules/yarn/bin/yarn.js")
        .to_string_lossy()
        .into_owned()
}
fn bootstrap(f: Fixture, slot: &Path, root: &Path, log: &Path) -> Result<f64> {
    let start = Instant::now();
    match f.name {
        "rust" => {
            must(slot, log, "cargo", &["fetch", "--locked"])?;
        }
        "python" => {
            if !slot.join(".venv").exists() {
                must(slot, log, "uv", &["venv", "--python", "3.12"])?;
            }
            must(
                slot,
                log,
                "uv",
                &[
                    "pip",
                    "install",
                    "--python",
                    &python(slot),
                    "-r",
                    "requirements/tests.txt",
                    "flit_core==3.9.0",
                ],
            )?;
            must(
                slot,
                log,
                "uv",
                &[
                    "pip",
                    "install",
                    "--python",
                    &python(slot),
                    "--no-build-isolation",
                    "-e",
                    ".",
                ],
            )?;
        }
        _ => {
            must(
                slot,
                log,
                "node",
                &[
                    &yarn(root),
                    "install",
                    "--frozen-lockfile",
                    "--non-interactive",
                    "--ignore-scripts",
                ],
            )?;
        }
    };
    Ok(start.elapsed().as_secs_f64())
}
fn gate(f: Fixture, slot: &Path, root: &Path, log: &Path) -> Result<(bool, f64)> {
    match f.name {
        "rust" => run(slot, log, "cargo", &["test", "--locked", "--workspace"]),
        "python" => run(slot, log, &python(slot), &["-m", "pytest"]),
        _ => run(
            slot,
            log,
            "node",
            &[&yarn(root), "run", "test:ts-jest", "--runInBand"],
        ),
    }
}
fn input_key(slot: &Path, f: Fixture) -> Result<Vec<u8>> {
    let mut bytes = fs::read(slot.join(f.input))?;
    for name in ["Cargo.lock", "yarn.lock", "requirements/tests.txt"] {
        if slot.join(name).exists() {
            bytes.extend(fs::read(slot.join(name))?);
        }
    }
    Ok(bytes)
}
fn median(values: &[f64]) -> Result<f64> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err("invalid timing sample".into());
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    Ok(if v.len() % 2 == 0 {
        (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.
    } else {
        v[v.len() / 2]
    })
}
fn overhead(raw: &[f64], warm: &[f64]) -> Result<f64> {
    let r = median(raw)?;
    if r == 0. {
        return Err("zero gate duration".into());
    }
    Ok((median(warm)? - r) / r)
}
fn prepare(f: Fixture, root: &Path, n: usize) -> Result<PathBuf> {
    let parent = root.join(format!("{}-{n}", f.name));
    fs::create_dir(&parent)?;
    fs::create_dir(parent.join("tmp"))?;
    let slot = parent.join("repo");
    fs::create_dir(&slot)?;
    let log = root.join(format!("{}-setup.log", f.name));
    must(&slot, &log, "git", &["init"])?;
    must(&slot, &log, "git", &["remote", "add", "origin", f.url])?;
    must(
        &slot,
        &log,
        "git",
        &["fetch", "--depth", "1", "origin", f.rev],
    )?;
    must(&slot, &log, "git", &["checkout", "--detach", "FETCH_HEAD"])?;
    must(&slot, &log, "git", &["rev-parse", "HEAD", "HEAD^{tree}"])?;
    Ok(slot)
}
fn reset(slot: &Path, log: &Path) -> Result<()> {
    must(slot, log, "git", &["reset", "--hard", "HEAD"])?;
    Ok(())
}
fn ecosystem(f: Fixture, root: &Path) -> Result<()> {
    let log = root.join(format!("{}.log", f.name));
    let slot = prepare(f, root, 0)?;
    let cold = bootstrap(f, &slot, root, &log)?;
    let mut samples = fs::File::create(root.join(format!("{}-samples.tsv", f.name)))?;
    writeln!(samples, "kind\tsample\tseconds\tpass")?;
    writeln!(samples, "bootstrap\t0\t{cold}\ttrue")?;
    let (ok, secs) = gate(f, &slot, root, &log)?;
    writeln!(samples, "cold-gate\t0\t{secs}\t{ok}")?;
    if !ok {
        return Err("cold gate failed".into());
    }
    let mut raw = vec![];
    let mut warm = vec![];
    for i in 0..5 {
        let (ok, t) = gate(f, &slot, root, &log)?;
        writeln!(samples, "raw\t{i}\t{t}\t{ok}")?;
        if !ok {
            return Err("raw gate failed".into());
        }
        raw.push(t);
        let start = Instant::now();
        reset(&slot, &log)?;
        // Alternating a harmless tracked source edit forces real incremental recompilation.
        let source = slot.join(f.poison);
        let mut text = fs::read_to_string(&source)?;
        text.push_str(if f.name == "python" {
            "\n# S3 warm candidate\n"
        } else {
            "\n// S3 warm candidate\n"
        });
        text.push_str(&format!(
            "{} candidate {i}\n",
            if f.name == "python" { "#" } else { "//" }
        ));
        fs::write(source, text)?;
        let (ok, _) = gate(f, &slot, root, &log)?;
        let t = start.elapsed().as_secs_f64();
        writeln!(samples, "warm\t{i}\t{t}\t{ok}")?;
        if !ok {
            return Err("warm gate failed".into());
        }
        warm.push(t);
    }
    let ratio = overhead(&raw, &warm)?;
    let mut result = fs::File::create(root.join(format!("{}-result.log", f.name)))?;
    writeln!(
        result,
        "raw_median_s={} warm_median_s={} overhead={} criterion_lt_0.15={}",
        median(&raw)?,
        median(&warm)?,
        ratio,
        ratio < 0.15
    )?;
    reset(&slot, &log)?;
    let before = input_key(&slot, f)?;
    let input = slot.join(f.input);
    let mut content = fs::read_to_string(&input)?;
    if f.name == "typescript" {
        content = content.replacen("\"version\": \"3.24.2\"", "\"version\": \"3.24.3\"", 1);
    } else {
        content.push_str("\n# S3 bootstrap-input invalidation\n");
    }
    fs::write(input, content)?;
    if before == input_key(&slot, f)? {
        return Err("invalidation fixture did not change inputs".into());
    }
    let t = bootstrap(f, &slot, root, &log)?;
    writeln!(samples, "invalidated-bootstrap\t0\t{t}\ttrue")?;
    writeln!(result, "input_change_detected=true bootstrap_reran=true")?;
    reset(&slot, &log)?;
    // A broken source must fail despite warm build/test caches.
    fs::write(
        slot.join(f.poison),
        "THIS IS DELIBERATELY INVALID SOURCE !!!\n",
    )?;
    let (bad, _) = gate(f, &slot, root, &log)?;
    writeln!(result, "stale_cache_rejects_broken_source={}", !bad)?;
    if bad {
        return Err("stale cache masked broken source".into());
    }
    reset(&slot, &log)?;
    fs::write(slot.join("s3-untracked-residue"), "residue")?;
    fs::write(slot.join("s3-ignored-residue"), "residue")?;
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(slot.join(".git/info/exclude"))?
        .write_all(b"\ns3-ignored-residue\n")?;
    must(
        &slot,
        &log,
        "git",
        &["clean", "-fdx", "-e", ".venv", "-e", "node_modules"],
    )?;
    if slot.join("s3-untracked-residue").exists() || slot.join("s3-ignored-residue").exists() {
        return Err("residue persisted".into());
    }
    writeln!(result, "ignored_and_untracked_residue_removed=true")?;
    let second = prepare(f, root, 1)?;
    bootstrap(f, &second, root, &log)?;
    let root2 = root.to_path_buf();
    let s2 = second.clone();
    let log2 = root.join(format!("{}-concurrent-1.log", f.name));
    let handle = thread::spawn(move || gate(f, &s2, &root2, &log2));
    let first = gate(
        f,
        &slot,
        root,
        &root.join(format!("{}-concurrent-0.log", f.name)),
    )?;
    let second_result = handle.join().map_err(|_| "slot thread panicked")??;
    writeln!(
        result,
        "concurrent_slot_0={} concurrent_slot_1={} private_temp_and_build_paths=true",
        first.0, second_result.0
    )?;
    if !first.0 || !second_result.0 {
        return Err("concurrent gate failed".into());
    }
    if ratio >= 0.15 {
        return Err(format!(
            "{} warm overhead {ratio:.3} fails unchanged <15% criterion",
            f.name
        )
        .into());
    }
    Ok(())
}
fn isolation(root: &Path) -> Result<()> {
    let mut log = fs::File::create(root.join("isolation.log"))?;
    let first = TcpListener::bind("127.0.0.1:0")?;
    let addr = first.local_addr()?;
    let collision = TcpListener::bind(addr).is_err();
    let second = TcpListener::bind("127.0.0.1:0")?;
    if !collision || second.local_addr()? == addr {
        return Err("port isolation failed".into());
    }
    writeln!(
        log,
        "port_collision_detected=true retry_ephemeral_port=true first={} second={}",
        addr,
        second.local_addr()?
    )?;
    let mut children = Vec::new();
    for n in 0..2 {
        let path = root.join(format!("port-slot-{n}.log"));
        let child = Command::new(env::current_exe()?)
            .arg("port-worker")
            .env("MAI_SLOT", n.to_string())
            .env("MAI_PORT_BASE", addr.port().to_string())
            .stdout(Stdio::from(fs::File::create(&path)?))
            .spawn()?;
        children.push((child, path));
    }
    let mut allocated = Vec::new();
    for (mut child, path) in children {
        if !child.wait()?.success() {
            return Err("port worker failed".into());
        }
        let output = fs::read_to_string(path)?;
        if !output.contains("collision=true") {
            return Err("worker did not observe collision".into());
        }
        allocated.push(
            output
                .lines()
                .last()
                .ok_or("missing bound port")?
                .to_string(),
        );
    }
    if allocated[0] == allocated[1] {
        return Err("workers reused occupied port".into());
    }
    writeln!(
        log,
        "subprocess_slot_collision_retry=true distinct_ports=true"
    )?;
    // Portable safe copy always tested; COW is a capability probe, not assumed.
    let source = root.join("template");
    fs::write(&source, b"original")?;
    let safe = root.join("safe-copy");
    fs::copy(&source, &safe)?;
    fs::write(&safe, b"changed")?;
    if fs::read(&source)? != b"original" {
        return Err("copy isolation failed".into());
    }
    let cow = root.join("cow-copy");
    let status = if cfg!(target_os = "linux") {
        Command::new("cp")
            .arg("--reflink=always")
            .arg(&source)
            .arg(&cow)
            .output()
            .ok()
    } else if cfg!(target_os = "macos") {
        Command::new("cp")
            .arg("-c")
            .arg(&source)
            .arg(&cow)
            .output()
            .ok()
    } else {
        None
    };
    let available = status.as_ref().is_some_and(|o| o.status.success());
    if available {
        fs::write(cow, b"changed")?;
        if fs::read(&source)? != b"original" {
            return Err("COW isolation failed".into());
        }
    }
    writeln!(
        log,
        "cow_available={available} safe_copy_isolation=true windows_cow_probe_not_implemented={}",
        cfg!(windows)
    )?;
    cancellation(root)?;
    Ok(())
}
fn terminate_tree(id: u32) -> Result<()> {
    #[cfg(unix)]
    let status = Command::new("kill")
        .args(["-KILL", "--", &format!("-{id}")])
        .status()?;
    #[cfg(windows)]
    let status = Command::new("taskkill")
        .args(["/PID", &id.to_string(), "/T", "/F"])
        .status()?;
    if !status.success() {
        return Err("process-tree termination failed".into());
    }
    Ok(())
}
fn cancellation(root: &Path) -> Result<()> {
    let executable = env::current_exe()?;
    let heartbeat = root.join("heartbeat");
    let mut cmd = Command::new(&executable);
    cmd.arg("worker")
        .arg(&heartbeat)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut worker = cmd.spawn()?;
    for _ in 0..100 {
        if heartbeat.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !heartbeat.exists() {
        let _ = terminate_tree(worker.id());
        let _ = worker.wait();
        return Err("descendant failed to start".into());
    }
    terminate_tree(worker.id())?;
    worker.wait()?;
    thread::sleep(Duration::from_millis(150));
    let before = fs::read(&heartbeat)?;
    thread::sleep(Duration::from_millis(250));
    let after = fs::read(&heartbeat)?;
    if before != after {
        return Err("descendant survived cancellation".into());
    }
    fs::write(
        root.join("cancellation.log"),
        "parent_reaped=true descendant_heartbeat_stopped=true\n",
    )?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("port-worker") {
        let port: u16 = env::var("MAI_PORT_BASE")?.parse()?;
        let requested = TcpListener::bind(("127.0.0.1", port));
        let collision = requested.is_err();
        let listener = requested.or_else(|_| TcpListener::bind("127.0.0.1:0"))?;
        println!(
            "slot={} collision={collision}\n{}",
            env::var("MAI_SLOT")?,
            listener.local_addr()?.port()
        );
        thread::sleep(Duration::from_millis(300));
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("worker") {
        let mut child = Command::new(env::current_exe()?)
            .arg("heartbeat")
            .arg(&args[2])
            .spawn()?;
        child.wait()?;
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("heartbeat") {
        let mut n = 0u64;
        loop {
            fs::write(&args[2], n.to_string())?;
            n += 1;
            thread::sleep(Duration::from_millis(20));
        }
    }
    let root = PathBuf::from(args.get(2).ok_or("usage: s3 run NEW_ROOT")?);
    if root.exists() {
        return Err("root already exists".into());
    }
    fs::create_dir_all(&root)?;
    let root = external_root(fs::canonicalize(root)?, cfg!(windows))?;
    isolation(&root)?;
    if args.get(1).map(String::as_str) == Some("isolation") {
        return Ok(());
    }
    let setup = root.join("tools.log");
    fs::create_dir(root.join("tmp"))?;
    for tool in ["git", "cargo", "rustc", "uv", "node", "npm"] {
        let _ = run(&root, &setup, tool, &["--version"]);
    }
    let tools = root.join("tools");
    fs::create_dir(&tools)?;
    let yarn_ready = must(
        &tools,
        &setup,
        "npm",
        &[
            "install",
            "--prefix",
            &tools.to_string_lossy(),
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "yarn@1.22.22",
        ],
    )
    .is_ok();
    let mut failures = vec![];
    for f in FIXTURES {
        if f.name == "typescript" && !yarn_ready {
            failures.push("typescript: pinned yarn setup failed".into());
            continue;
        }
        if let Err(e) = ecosystem(f, &root) {
            let message = format!("{}: {e}", f.name);
            fs::write(root.join(format!("{}-failure.log", f.name)), &message)?;
            failures.push(message);
        }
    }
    fs::write(
        root.join("summary.log"),
        format!("failures={failures:#?}\nplatform={}\n", env::consts::OS),
    )?;
    if !failures.is_empty() {
        return Err(format!("S3 remains NO-GO: {failures:?}").into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_local_roots_drop_extended_prefix() {
        assert_eq!(
            windows_drive_path(r"\\?\C:\fixtures\space here").unwrap(),
            "C:/fixtures/space here"
        );
        assert_eq!(
            windows_drive_path(r"D:\fixtures\run").unwrap(),
            "D:/fixtures/run"
        );
        assert_eq!(
            windows_drive_path("C:/fixtures/run").unwrap(),
            "C:/fixtures/run"
        );
    }
    #[test]
    fn windows_unc_and_device_roots_are_rejected() {
        for path in [
            r"\\?\UNC\host\share",
            r"\\host\share",
            "//host/share",
            r"\\.\C:\fixtures",
            "C:relative",
            "relative",
        ] {
            assert!(
                windows_drive_path(path).is_err(),
                "accepted unsupported root {path}"
            );
        }
    }
    #[test]
    fn unix_roots_preserve_literal_backslashes() {
        let path = PathBuf::from(r"/tmp/fixture\literal");
        assert_eq!(external_root(path.clone(), false).unwrap(), path);
    }
    #[test]
    fn threshold_is_strict() {
        assert_eq!(overhead(&[100.], &[115.]).unwrap(), 0.15);
        assert!(overhead(&[100.], &[114.]).unwrap() < 0.15);
    }
    #[test]
    fn invalid_measurements_rejected() {
        assert!(median(&[]).is_err());
        assert!(median(&[f64::NAN]).is_err());
        assert!(median(&[-1.]).is_err());
        assert!(overhead(&[0.], &[1.]).is_err());
    }
    #[test]
    fn median_resists_outlier() {
        assert_eq!(median(&[2., 100., 1.]).unwrap(), 2.);
        assert_eq!(median(&[1., 3.]).unwrap(), 2.);
    }
    #[test]
    fn bootstrap_inputs_are_declared() {
        for f in FIXTURES {
            assert_eq!(f.rev.len(), 40);
            assert!(!f.input.is_empty());
        }
    }
    #[test]
    fn command_failures_are_not_passes() {
        let root = PathBuf::from(format!(".throwaway/s3-unit-errors-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let root = external_root(fs::canonicalize(root).unwrap(), cfg!(windows)).unwrap();
        assert!(run(&root, &root.join("spawn.log"), "s3-no-such-executable", &[]).is_err());
        assert!(
            !run(
                &root,
                &root.join("exit.log"),
                "rustc",
                &["--s3-invalid-flag"]
            )
            .unwrap()
            .0
        );
    }
    #[test]
    fn private_slot_paths() {
        let c = command(Path::new("fixtures/slot-1/repo"), "cargo", &["test"]);
        let vars: Vec<_> = c.get_envs().collect();
        assert!(vars.iter().any(
            |(k, v)| *k == "CARGO_TARGET_DIR" && v.unwrap() == "fixtures/slot-1/repo/../target"
        ));
    }
}
