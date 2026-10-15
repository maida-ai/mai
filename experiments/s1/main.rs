//! Disposable S1 experiment. No mai library, daemon, database, or product defaults.
mod scenarios;
mod transport;
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
const ZERO: &str = "0000000000000000000000000000000000000000";
fn command(program: &str) -> Command {
    let mut c = Command::new(program);
    for (key, _) in env::vars() {
        if key.starts_with("GIT_") {
            c.env_remove(key);
        }
    }
    c.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.autocrlf")
        .env("GIT_CONFIG_VALUE_0", "false")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "S1 Fixture")
        .env("GIT_AUTHOR_EMAIL", "s1@example.invalid")
        .env("GIT_COMMITTER_NAME", "S1 Fixture")
        .env("GIT_COMMITTER_EMAIL", "s1@example.invalid")
        .env("GIT_EDITOR", "true");
    c
}
fn git(repo: &Path, args: &[&str]) -> Output {
    command("git")
        .arg("-C")
        .arg(git_path(repo))
        .args(args)
        .output()
        .unwrap()
}
// Git for Windows and its hook shell accept drive paths with forward slashes,
// not Rust's canonical extended-length prefix. Unix filenames stay untouched.
fn windows_git_path(path: &str) -> String {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    assert!(
        !path.starts_with("UNC\\"),
        "use a local drive for the fixture"
    );
    path.replace('\\', "/")
}
fn git_path(path: &Path) -> String {
    let path = path.to_str().expect("UTF-8 fixture path");
    if cfg!(windows) {
        windows_git_path(path)
    } else {
        path.into()
    }
}
fn file_url(path: &str) -> String {
    let encoded: String = path
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/:-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!(
        "file://{}{encoded}",
        if path.starts_with('/') { "" } else { "/" }
    )
}
fn text(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn g(repo: &Path, args: &[&str]) -> String {
    text(git(repo, args))
}
fn audit(repo: &Path, line: &str) {
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(repo.join("s1-audit.log"))
            .unwrap(),
        "{line}"
    )
    .unwrap();
}
fn engine(repo: &Path, oid: &str) -> Result<String, String> {
    if repo.join("engine-down").exists() {
        return Err("ENGINE_UNAVAILABLE".into());
    }
    if !git(repo, &["cat-file", "-e", &format!("{oid}^{{commit}}")])
        .status
        .success()
    {
        return Err("OBJECT_NOT_VISIBLE".into());
    }
    audit(repo, &format!("engine-visible {oid}"));
    let retained = format!("refs/mai/changes/{oid}"); // Fixture IDs only; not the proposed numeric schema.
    if g(repo, &["for-each-ref", "--format=%(objectname)", &retained]).is_empty() {
        if !git(repo, &["update-ref", &retained, oid, ZERO])
            .status
            .success()
        {
            return Err("REF_CAS_FAILED".into());
        }
    }
    audit(repo, &format!("engine-retained {oid}"));
    if repo.join("crash-after-ref").exists() {
        return Err("INTERRUPTED_AFTER_REF".into());
    }
    fs::create_dir_all(repo.join("queue")).unwrap();
    let entry = repo.join("queue").join(oid);
    if !entry.exists() {
        fs::write(entry, "queued\n").unwrap();
    }
    audit(repo, &format!("engine-queued {oid}"));
    Ok(retained)
}
fn register(repo: &Path, oid: &str) -> Result<String, String> {
    // A distinct process with quarantine/location variables removed, like an engine
    // that did not inherit receive-pack's object access environment.
    let out = command(env::current_exe().unwrap().to_str().unwrap())
        .args(["engine", repo.to_str().unwrap(), oid])
        .output()
        .unwrap();
    if out.status.success() {
        Ok(String::from_utf8(out.stdout).unwrap().trim().into())
    } else {
        Err(String::from_utf8(out.stderr).unwrap().trim().into())
    }
}
fn options(opts: &[String], authenticated: Option<&str>) -> Result<String, &'static str> {
    // Deliberately provisional fixture grammar/limits, not shipping policy.
    if opts.len() > 16 || opts.iter().map(String::len).sum::<usize>() > 4096 {
        return Err("OPTION_LIMIT");
    }
    let mut values = std::collections::BTreeMap::new();
    for opt in opts {
        if opt.len() > 1024 {
            return Err("OPTION_LIMIT");
        }
        let (key, value) = opt.split_once('=').ok_or("INVALID_OPTION")?;
        if !matches!(
            key,
            "mai.agent" | "mai.priority" | "mai.fixes" | "mai.touches" | "mai.wait"
        ) {
            return Err("UNKNOWN_OPTION");
        }
        if value.is_empty() || value.bytes().any(|b| b.is_ascii_control()) {
            return Err("INVALID_OPTION");
        }
        if values.insert(key, value).is_some() {
            return Err("DUPLICATE_OPTION");
        }
    }
    let declared = values.get("mai.agent").copied().unwrap_or("alice");
    if !matches!(declared, "alice" | "bob") {
        return Err("INVALID_IDENTITY");
    }
    if authenticated.is_some_and(|a| a != declared) {
        return Err("IDENTITY_MISMATCH");
    }
    let priority = values.get("mai.priority").copied().unwrap_or("normal");
    if !matches!(priority, "normal" | "low" | "high" | "sev") {
        return Err("INVALID_PRIORITY");
    }
    if let Some(fixes) = values.get("mai.fixes") {
        if *fixes != "incident-1" {
            return Err("INACTIVE_INCIDENT");
        }
        // Synthetic active incident grants only authenticated Alice this scoped fix.
        if authenticated != Some("alice") || priority != "sev" {
            return Err("GRANT_REQUIRED");
        }
    } else if matches!(priority, "high" | "sev") {
        return Err("GRANT_REQUIRED");
    }
    if let Some(touches) = values.get("mai.touches") {
        if touches.split(',').any(|s| s.is_empty() || s.contains(' ')) {
            return Err("INVALID_TOUCHES");
        }
    }
    if values
        .get("mai.wait")
        .is_some_and(|v| *v != "true" && *v != "false")
    {
        return Err("INVALID_WAIT");
    }
    Ok(declared.into())
}
fn policy(reference: &str, oid: &str, agent: &str, fallback: bool) -> Result<(), &'static str> {
    if oid == ZERO {
        return Err("DELETE_FORBIDDEN");
    }
    if reference == format!("refs/drafts/{agent}") {
        return Ok(());
    }
    if !fallback && reference == "refs/for/main" {
        return Ok(());
    }
    if fallback && reference == format!("refs/incoming/{agent}/{oid}") {
        return Ok(());
    }
    Err("REF_FORBIDDEN")
}
fn packet(r: &mut impl Read) -> Vec<u8> {
    let mut hdr = [0; 4];
    r.read_exact(&mut hdr).unwrap();
    let n = usize::from_str_radix(std::str::from_utf8(&hdr).unwrap(), 16).unwrap();
    if n == 0 {
        return vec![];
    }
    assert!((4..=65520).contains(&n));
    let mut data = vec![0; n - 4];
    r.read_exact(&mut data).unwrap();
    data
}
fn packets(r: &mut impl Read) -> Vec<String> {
    let mut rows = vec![];
    loop {
        let p = packet(r);
        if p.is_empty() {
            return rows;
        }
        rows.push(String::from_utf8(p).unwrap().trim_end_matches('\n').into());
    }
}
fn send(w: &mut impl Write, s: &str) {
    write!(w, "{:04x}{s}", s.len() + 4).unwrap();
}
fn flush(w: &mut impl Write) {
    w.write_all(b"0000").unwrap();
    w.flush().unwrap();
}
fn proc_receive(repo: &Path) {
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let version = packets(&mut input).join("");
    assert!(version.starts_with("version=1"));
    let push_options = version.contains("push-options");
    send(
        &mut output,
        if push_options {
            "version=1\0push-options"
        } else {
            "version=1"
        },
    );
    flush(&mut output);
    let rows = packets(&mut input);
    let opts = if push_options {
        packets(&mut input)
    } else {
        vec![]
    };
    let authenticated = env::var("S1_AUTH_AGENT").ok();
    let agent = options(&opts, authenticated.as_deref());
    for row in rows {
        let parts: Vec<_> = row.split_whitespace().collect();
        assert_eq!(parts.len(), 3);
        let (old, oid, reference) = (parts[0], parts[1], parts[2]);
        let result = agent.as_ref().map_err(|e| e.to_string()).and_then(|a| {
            policy(reference, oid, a, false).map_err(str::to_owned)?;
            if reference.starts_with("refs/drafts/") {
                if !git(repo, &["update-ref", reference, oid, old])
                    .status
                    .success()
                {
                    return Err("DRAFT_CAS_FAILED".into());
                }
                Ok(reference.into())
            } else {
                register(repo, oid)
            }
        });
        match result {
            Ok(retained) => {
                if repo.join("lose-reply").exists() {
                    std::process::exit(74);
                }
                if repo.join("pause-reply").exists() {
                    fs::write(repo.join("reply-ready"), oid).unwrap();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !repo.join("release-reply").exists() {
                        assert!(std::time::Instant::now() < deadline);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
                audit(repo, &format!("hook-ok {oid} {reference}"));
                send(&mut output, &format!("ok {reference}"));
                send(&mut output, &format!("option refname {retained}"));
                send(&mut output, &format!("option new-oid {oid}"));
            }
            Err(reason) => send(&mut output, &format!("ng {reference} {reason}")),
        }
    }
    flush(&mut output);
}
fn receive_lines() -> Vec<String> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    s.lines().map(str::to_owned).collect()
}
fn pre_receive(repo: &Path) {
    let rows = receive_lines();
    // Probe only: fresh quarantined objects are invisible outside the hook env.
    for row in &rows {
        let oid = row.split_whitespace().nth(1).unwrap();
        if oid != ZERO {
            let inherited = Command::new("git")
                .args(["cat-file", "-e", oid])
                .output()
                .unwrap()
                .status
                .success();
            let clean = git(repo, &["cat-file", "-e", oid]).status.success();
            audit(
                repo,
                &format!("pre-visible inherited={inherited} separate={clean} {oid}"),
            );
        }
    }
    if !repo.join("fallback").exists() {
        return;
    }
    let n: usize = env::var("GIT_PUSH_OPTION_COUNT")
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    if n > 16 {
        eprintln!("OPTION_LIMIT");
        std::process::exit(1);
    }
    let opts: Vec<_> = (0..n)
        .map(|i| env::var(format!("GIT_PUSH_OPTION_{i}")).unwrap())
        .collect();
    let authenticated = env::var("S1_AUTH_AGENT").ok();
    let result = options(&opts, authenticated.as_deref()).and_then(|agent| {
        for row in rows {
            let p: Vec<_> = row.split_whitespace().collect();
            policy(p[2], p[1], &agent, true)?;
        }
        Ok(agent)
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn post_receive(repo: &Path) {
    for row in receive_lines() {
        let p: Vec<_> = row.split_whitespace().collect();
        if p[2].starts_with("refs/incoming/") {
            match register(repo, p[1]) {
                Ok(_) => eprintln!("QUEUED {}", p[1]),
                Err(e) => {
                    eprintln!("RETAINED_PENDING_REGISTRATION {} {e}", p[1]);
                    std::process::exit(1);
                }
            }
        }
    }
}
fn main() {
    let args: Vec<_> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("run") => scenarios::run(Path::new(
            args.get(2).expect("new output directory required"),
        )),
        Some("proc") => proc_receive(&env::current_dir().unwrap()),
        Some("pre") => pre_receive(&env::current_dir().unwrap()),
        Some("post") => post_receive(&env::current_dir().unwrap()),
        Some("engine") => match engine(Path::new(&args[2]), &args[3]) {
            Ok(r) => println!("{r}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        _ => panic!("usage: s1 run NEW_DIRECTORY"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_windows_paths_and_file_urls() {
        assert_eq!(
            windows_git_path(r"\\?\C:\fixture dir\s1.exe"),
            "C:/fixture dir/s1.exe"
        );
        assert_eq!(
            file_url("C:/fixture dir/a#b"),
            "file:///C:/fixture%20dir/a%23b"
        );
        assert_eq!(
            file_url("/tmp/fixture dir/a%20b"),
            "file:///tmp/fixture%20dir/a%2520b"
        );
    }
    fn opts(s: &[&str], auth: Option<&str>) -> Result<String, &'static str> {
        options(&s.iter().map(|s| s.to_string()).collect::<Vec<_>>(), auth)
    }
    #[test]
    fn options_are_bounded_and_typed() {
        assert_eq!(
            opts(
                &["mai.priority=low", "mai.touches=a,b::c", "mai.wait=true"],
                None
            ),
            Ok("alice".into())
        );
        for (input, error) in [
            ("mai.priority=sev", "GRANT_REQUIRED"),
            ("mai.priority=root", "INVALID_PRIORITY"),
            ("mai.fixes=closed", "INACTIVE_INCIDENT"),
            ("mai.grant=sev", "UNKNOWN_OPTION"),
            ("mai.touches=a,,b", "INVALID_TOUCHES"),
        ] {
            assert_eq!(opts(&[input], None), Err(error));
        }
        assert_eq!(
            opts(&["mai.agent=bob"], Some("alice")),
            Err("IDENTITY_MISMATCH")
        );
        assert_eq!(
            opts(&["mai.priority=sev", "mai.fixes=incident-1"], Some("alice")),
            Ok("alice".into())
        );
        assert_eq!(
            opts(&["mai.priority=sev", "mai.fixes=incident-1"], None),
            Err("GRANT_REQUIRED")
        );
        assert_eq!(
            opts(&["mai.wait=true", "mai.wait=false"], None),
            Err("DUPLICATE_OPTION")
        );
        assert_eq!(
            options(&vec!["mai.wait=true".into(); 17], None),
            Err("OPTION_LIMIT")
        );
        assert_eq!(
            options(&[format!("mai.touches={}", "a".repeat(1024))], None),
            Err("OPTION_LIMIT")
        );
    }
    #[test]
    fn refs_are_allowlisted() {
        assert!(policy("refs/for/main", "abc", "alice", false).is_ok());
        assert!(policy("refs/drafts/alice", "abc", "alice", false).is_ok());
        for r in [
            "refs/heads/main",
            "refs/mai/changes/1",
            "refs/drafts/bob",
            "refs/for/other",
        ] {
            assert_eq!(policy(r, "abc", "alice", false), Err("REF_FORBIDDEN"));
        }
        assert_eq!(
            policy("refs/drafts/alice", ZERO, "alice", false),
            Err("DELETE_FORBIDDEN")
        );
    }
}
