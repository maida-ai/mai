//! Isolated, standard-library-only lockfile experiment; no mai implementation.
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn windows_path(path: &str) -> String {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    assert!(
        !path.starts_with("UNC\\") && !path.starts_with(r"\\"),
        "use a local Windows drive"
    );
    path.replace('\\', "/")
}
fn external_path(path: PathBuf) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(windows_path(path.to_str().expect("UTF-8 fixture path")))
    } else {
        path
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Evidence {
    tree: String,
    parent: String,
    policy: String,
    attempt: u32,
    passed: bool,
}
fn accepts(e: &Evidence, tree: &str, parent: &str, policy: &str, attempt: u32) -> bool {
    e.passed && e.tree == tree && e.parent == parent && e.policy == policy && e.attempt == attempt
}
fn trusted(base: &[u8], candidate: &[u8]) -> bool {
    base == candidate
}
fn put(root: &Path, name: &str, contents: &str) {
    let p = root.join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, contents).unwrap();
}
struct Harness {
    root: PathBuf,
    seq: usize,
}
impl Harness {
    fn run(
        &mut self,
        cwd: &Path,
        tool: &str,
        args: &[&str],
        timeout: Duration,
    ) -> (bool, String, bool) {
        self.seq += 1;
        let log = self.root.join(format!("command-{:04}.log", self.seq));
        let mut f = fs::File::create(&log).unwrap();
        writeln!(
            f,
            "cwd={}\ncommand={tool} {args:?}\ntimeout_ms={}",
            cwd.display(),
            timeout.as_millis()
        )
        .unwrap();
        let start = Instant::now();
        #[cfg(windows)]
        let mut cmd = if tool == "npm" {
            let mut c = Command::new("cmd");
            c.args(["/d", "/s", "/c", "npm.cmd"]);
            c
        } else {
            Command::new(tool)
        };
        #[cfg(not(windows))]
        let mut cmd = Command::new(tool);
        cmd.args(args)
            .current_dir(cwd)
            .env("CARGO_HOME", self.root.join("cargo-home"))
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .env("UV_CACHE_DIR", self.root.join("uv-cache"))
            .env("UV_PYTHON_DOWNLOADS", "never")
            .env("npm_config_cache", self.root.join("npm-cache"))
            .env("GIT_CONFIG_GLOBAL", self.root.join("empty-git-config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdout(Stdio::from(f.try_clone().unwrap()))
            .stderr(Stdio::from(f.try_clone().unwrap()));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let mut child = cmd.spawn().unwrap_or_else(|e| panic!("spawn {tool}: {e}"));
        let mut timed_out = false;
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if start.elapsed() > timeout {
                timed_out = true;
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-KILL", "--", &format!("-{}", child.id())])
                        .status();
                }
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .status();
                }
                let _ = child.kill();
                break child.wait().unwrap();
            }
            thread::sleep(Duration::from_millis(10));
        };
        writeln!(
            f,
            "\nstatus={status}\ntimeout={timed_out}\nelapsed_ms={}",
            start.elapsed().as_millis()
        )
        .unwrap();
        let text = fs::read_to_string(log).unwrap();
        (status.success() && !timed_out, text, timed_out)
    }
    fn ok(&mut self, p: &Path, t: &str, a: &[&str]) -> String {
        let (ok, s, _) = self.run(p, t, a, Duration::from_secs(120));
        assert!(ok, "{s}");
        s
    }
    fn git(&mut self, p: &Path, a: &[&str]) -> String {
        let output = self.ok(p, "git", a);
        output
            .lines()
            .skip(3)
            .take_while(|line| !line.starts_with("status="))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn rev(&mut self, p: &Path, s: &str) -> String {
        self.git(p, &["rev-parse", s])
    }
    fn commit(&mut self, p: &Path, m: &str) {
        self.ok(p, "git", &["add", "."]);
        self.ok(p, "git", &["commit", "-m", m]);
    }
}
fn manifest(kind: &str, a: bool, b: bool) -> String {
    let pad = "\n# independent edit separation\n".repeat(8);
    match kind {
        "cargo" => format!(
            "[package]\nname='s6-fixture'\nversion='0.0.1'\nedition='2021'\n[dependencies]\n{}{}{}",
            if a {
                "alpha={path='vendor/alpha',version='=1.0.0'}\n"
            } else {
                "# alpha pending\n"
            },
            pad,
            if b {
                "beta={path='vendor/beta',version='=1.0.0'}\n"
            } else {
                "# beta pending\n"
            }
        ),
        "npm" => format!(
            "{{\n  \"name\": \"s6-fixture\",\n  \"version\": \"0.0.1\",\n  \"dependencies\": {{ {} }},\n{}  \"optionalDependencies\": {{ {} }}\n}}\n",
            if a {
                "\"alpha\": \"file:vendor/alpha\""
            } else {
                ""
            },
            "\n".repeat(12),
            if b {
                "\"beta\": \"file:vendor/beta\""
            } else {
                ""
            }
        ),
        "uv" => format!(
            "[project]\nname='s6-fixture'\nversion='0.0.1'\nrequires-python='>=3.10'\ndependencies=[\n{}{}{}]\n[tool.uv.sources]\nalpha={{path='vendor/alpha'}}\nbeta={{path='vendor/beta'}}\n",
            if a {
                "'alpha==1.0.0',\n"
            } else {
                "# alpha pending\n"
            },
            pad,
            if b {
                "'beta==1.0.0',\n"
            } else {
                "# beta pending\n"
            }
        ),
        _ => panic!("unknown ecosystem"),
    }
}
fn regenerate(h: &mut Harness, p: &Path, k: &str) -> bool {
    let (t, a): (&str, &[&str]) = match k {
        "cargo" => ("cargo", &["generate-lockfile", "--offline"]),
        "npm" => (
            "npm",
            &[
                "install",
                "--package-lock-only",
                "--offline",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ],
        ),
        _ => ("uv", &["lock", "--offline"]),
    };
    h.run(p, t, a, Duration::from_secs(120)).0
}
fn gate(h: &mut Harness, p: &Path, k: &str) -> bool {
    match k {
        "cargo" => {
            h.run(
                p,
                "cargo",
                &["run", "--locked", "--offline"],
                Duration::from_secs(120),
            )
            .0
        }
        "npm" => {
            h.run(
                p,
                "npm",
                &[
                    "ci",
                    "--offline",
                    "--ignore-scripts",
                    "--no-audit",
                    "--no-fund",
                ],
                Duration::from_secs(120),
            )
            .0 && h.run(p, "node", &["gate.js"], Duration::from_secs(120)).0
        }
        _ => {
            h.run(
                p,
                "uv",
                &["sync", "--locked", "--offline"],
                Duration::from_secs(120),
            )
            .0 && h
                .run(
                    p,
                    "uv",
                    &["run", "--no-sync", "--offline", "python", "gate.py"],
                    Duration::from_secs(120),
                )
                .0
        }
    }
}
fn policy_allowed(h: &mut Harness, p: &Path, base: &str, script: &str) -> bool {
    let mut files = vec!["trusted-policy", script];
    if p.join("vendor/alpha/backend.py").exists() {
        files.extend(["vendor/alpha/backend.py", "vendor/beta/backend.py"]);
    }
    for file in files {
        let approved = h.git(p, &["rev-parse", &format!("{base}:{file}")]);
        let candidate = h.git(p, &["hash-object", "--", file]);
        if !trusted(approved.as_bytes(), candidate.as_bytes()) {
            return false;
        }
    }
    true
}

const BACKEND: &str = include_str!("wheel_backend.py");
fn scenario(h: &mut Harness, k: &str) {
    let p = h.root.join(k);
    fs::create_dir(&p).unwrap();
    let (manifest_name, lock, gatefile, good) = match k {
        "cargo" => (
            "Cargo.toml",
            "Cargo.lock",
            "src/main.rs",
            "fn main(){assert_eq!(alpha::value(), 11); assert_eq!(beta::value(), 22);}\n",
        ),
        "npm" => (
            "package.json",
            "package-lock.json",
            "gate.js",
            "const assert=require('node:assert'); assert.equal(require('alpha'),11); assert.equal(require('beta'),22);\n",
        ),
        _ => (
            "pyproject.toml",
            "uv.lock",
            "gate.py",
            "import alpha, beta\nassert alpha.value == 11\nassert beta.value == 22\n",
        ),
    };
    put(
        &p,
        ".gitignore",
        "target/\nnode_modules/\n.venv/\n__pycache__/\n",
    );
    put(&p, manifest_name, &manifest(k, false, false));
    put(&p, gatefile, good);
    put(
        &p,
        "trusted-policy",
        "schema=1\nregenerate=offline\ngate=both-additions\n",
    );
    for (name, value) in [("alpha", 11), ("beta", 22)] {
        let v = format!("vendor/{name}");
        match k {
            "cargo" => {
                put(
                    &p,
                    &format!("{v}/Cargo.toml"),
                    &format!("[package]\nname='{name}'\nversion='1.0.0'\nedition='2021'\n"),
                );
                put(
                    &p,
                    &format!("{v}/src/lib.rs"),
                    &format!("pub fn value()->i32{{{value}}}\n"),
                );
            }
            "npm" => {
                put(
                    &p,
                    &format!("{v}/package.json"),
                    &format!(
                        "{{\"name\":\"{name}\",\"version\":\"1.0.0\",\"main\":\"index.js\"}}\n"
                    ),
                );
                put(
                    &p,
                    &format!("{v}/index.js"),
                    &format!("module.exports={value};\n"),
                );
            }
            _ => {
                put(
                    &p,
                    &format!("{v}/pyproject.toml"),
                    &format!(
                        "[build-system]\nrequires=[]\nbuild-backend='backend'\nbackend-path=['.']\n[project]\nname='{name}'\nversion='1.0.0'\n"
                    ),
                );
                put(&p, &format!("{v}/backend.py"), BACKEND);
                put(&p, &format!("{v}/{name}.py"), &format!("value = {value}\n"));
            }
        }
    }
    h.ok(&p, "git", &["init", "-b", "main"]);
    h.ok(&p, "git", &["config", "user.email", "s6@example.invalid"]);
    h.ok(&p, "git", &["config", "user.name", "S6 experiment"]);
    h.ok(&p, "git", &["config", "core.autocrlf", "false"]);
    assert!(regenerate(h, &p, k));
    h.commit(&p, "baseline fixture");
    let base = h.rev(&p, "HEAD");
    let trunk_lock = fs::read(p.join(lock)).unwrap();
    h.ok(&p, "git", &["checkout", "-b", "agent-alpha"]);
    put(&p, manifest_name, &manifest(k, true, false));
    assert!(regenerate(h, &p, k));
    h.commit(&p, "alpha addition and generated lock");
    h.ok(&p, "git", &["checkout", "-b", "agent-beta", &base]);
    put(&p, manifest_name, &manifest(k, false, true));
    assert!(regenerate(h, &p, k));
    h.commit(&p, "beta addition and generated lock");
    // Merge only manifests with real Git three-way semantics; locks are generated artifacts.
    h.ok(&p, "git", &["checkout", "-b", "candidate", &base]);
    h.ok(&p, "git", &["checkout", "agent-alpha", "--", manifest_name]);
    h.commit(&p, "alpha manifest");
    let alpha_manifest = h.rev(&p, "HEAD");
    h.ok(&p, "git", &["checkout", "-b", "beta-manifest", &base]);
    h.ok(&p, "git", &["checkout", "agent-beta", "--", manifest_name]);
    h.commit(&p, "beta manifest");
    h.ok(&p, "git", &["checkout", "candidate"]);
    h.ok(&p, "git", &["merge", "--no-edit", "beta-manifest"]);
    assert_eq!(fs::read(p.join(lock)).unwrap(), trunk_lock);
    let before = h.rev(&p, "HEAD^{tree}");
    let parent = h.rev(&p, "HEAD");
    assert!(policy_allowed(h, &p, &base, gatefile));
    assert!(regenerate(h, &p, k));
    let first = fs::read(p.join(lock)).unwrap();
    assert_ne!(first, trunk_lock);
    fs::write(p.join(lock), &trunk_lock).unwrap();
    assert!(regenerate(h, &p, k));
    assert_eq!(fs::read(p.join(lock)).unwrap(), first);
    assert!(policy_allowed(h, &p, &base, gatefile));
    h.ok(&p, "git", &["add", "-u"]);
    let checked_tree = h.git(&p, &["write-tree"]);
    assert!(gate(h, &p, k));
    h.ok(&p, "git", &["add", "-u"]);
    let tree = h.git(&p, &["write-tree"]);
    assert_ne!(tree, before);
    assert_eq!(tree, checked_tree, "gate changed tracked candidate bytes");
    let policy = h.git(&p, &["rev-parse", &format!("{base}:trusted-policy")]);
    let e = Evidence {
        tree: tree.clone(),
        parent: parent.clone(),
        policy: policy.clone(),
        attempt: 1,
        passed: true,
    };
    assert!(accepts(&e, &tree, &parent, &policy, 1));
    assert!(!accepts(&e, &before, &parent, &policy, 1));
    assert!(!accepts(&e, &tree, &base, &policy, 1));
    assert!(!accepts(&e, &tree, &parent, &policy, 2));
    // Policy and executable changes are independently rejected, not silently overlaid.
    let mut protected = vec!["trusted-policy", gatefile];
    if k == "uv" {
        protected.extend(["vendor/alpha/backend.py", "vendor/beta/backend.py"]);
    }
    for file in protected {
        let approved = fs::read(p.join(file)).unwrap();
        put(&p, file, "candidate bypass\n");
        assert!(!policy_allowed(h, &p, &base, gatefile));
        assert!(!trusted(&approved, &fs::read(p.join(file)).unwrap()));
        fs::write(p.join(file), approved).unwrap();
    }
    // Break actual dependency code: trusted gate must fail in every ecosystem.
    let broken = match k {
        "cargo" => "vendor/beta/src/lib.rs",
        "npm" => "vendor/beta/index.js",
        _ => "gate.py",
    };
    let old = fs::read(p.join(broken)).unwrap();
    put(
        &p,
        broken,
        match k {
            "cargo" => "pub fn value()->i32{99}\n",
            "npm" => "module.exports=99;\n",
            _ => "raise RuntimeError('injected gate failure')\n",
        },
    );
    assert!(!gate(h, &p, k));
    fs::write(p.join(broken), old).unwrap();
    // Genuine incompatible constraints, separate from offline availability probes.
    if k == "npm" {
        let path = "vendor/beta/package.json";
        let old = fs::read(p.join(path)).unwrap();
        put(
            &p,
            path,
            r#"{"name":"beta","version":"1.0.0","main":"index.js","peerDependencies":{"alpha":"2.0.0"}}"#,
        );
        fs::write(p.join(lock), &trunk_lock).unwrap();
        let (ok, output, _) = h.run(
            &p,
            "npm",
            &[
                "install",
                "--package-lock-only",
                "--offline",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
                "--strict-peer-deps",
                "--install-links",
            ],
            Duration::from_secs(120),
        );
        assert!(
            !ok,
            "incompatible peer constraints unexpectedly passed: {output}"
        );
        assert!(
            output.contains("npm error code ERESOLVE"),
            "expected peer solver contradiction: {output}"
        );
        put(&h.root, "npm-constraint.log", &output);
        fs::write(p.join(path), old).unwrap();
        fs::write(p.join(lock), &first).unwrap();
    }
    if k == "uv" {
        let path = "vendor/beta/pyproject.toml";
        let old = fs::read_to_string(p.join(path)).unwrap();
        put(&p, path, &format!("{old}requires-python='>=4'\n"));
        fs::write(p.join(lock), &trunk_lock).unwrap();
        let (ok, output, _) = h.run(&p, "uv", &["lock", "--offline"], Duration::from_secs(120));
        assert!(
            !ok,
            "incompatible Python constraints unexpectedly passed: {output}"
        );
        assert!(
            output.contains("No solution found"),
            "expected solver contradiction: {output}"
        );
        put(&h.root, "uv-constraint.log", &output);
        fs::write(p.join(path), old).unwrap();
        fs::write(p.join(lock), &first).unwrap();
    }
    let old = fs::read_to_string(p.join(manifest_name)).unwrap();
    let bad = match k {
        "npm" => old.replace("file:vendor/alpha", "999999.0.0"),
        "uv" => old
            .replacen("1.0.0", "999999.0.0", 1)
            .replace("alpha={path='vendor/alpha'}\n", ""),
        _ => old.replacen("1.0.0", "999999.0.0", 1),
    };
    put(&p, manifest_name, &bad);
    assert!(!regenerate(h, &p, k));
    put(&p, manifest_name, &old);
    fs::write(p.join(lock), &first).unwrap();
    assert!(gate(h, &p, k));
    let exe = external_path(env::current_exe().unwrap());
    let (_, _, timeout) = h.run(
        &p,
        exe.to_str().unwrap(),
        &["sleep"],
        Duration::from_millis(100),
    );
    assert!(timeout);
    h.ok(&p, "git", &["add", lock]);
    assert_eq!(h.git(&p, &["write-tree"]), tree);
    let landed = h.git(
        &p,
        &[
            "commit-tree",
            &tree,
            "-p",
            &parent,
            "-m",
            "accepted regenerated candidate\n\nMai-Revision: r1",
        ],
    );
    h.ok(
        &p,
        "git",
        &[
            "update-ref",
            "refs/heads/accepted",
            &landed,
            "0000000000000000000000000000000000000000",
        ],
    );
    assert_eq!(h.rev(&p, "accepted^{tree}"), e.tree);
    assert_eq!(h.rev(&p, "accepted^"), e.parent);
    put(
        &h.root,
        &format!("{k}-result.log"),
        &format!(
            "PASS ecosystem={k}\nbase={base}\nalpha_manifest={alpha_manifest}\npre_regeneration_tree={before}\nfinal_tree={tree}\nchecked_tree={checked_tree}\nparent={parent}\npolicy_blob={policy}\nattempt=1\nlanded={landed}\nchecks=independent-manifest-merge,trunk-lock-reset,offline-regeneration,byte-reproducibility,gate-tree-unchanged,both-imports,stale-tree-parent-attempt-rejection,policy-replacement,script-replacement,gate-failure,dependency-failure-offline,timeout,exact-landed-tree\nlimitations=local-source-version-pinned-fixtures;no-registry-or-production-qualification;timeout-probe-not-real-resolver-hang\n"
        ),
    );
    println!("PASS {k}");
}
fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "sleep") {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    assert_eq!(args.len(), 2, "usage: s6 NEW_ROOT");
    let root = PathBuf::from(&args[1]);
    assert!(!root.exists(), "new root required");
    fs::create_dir_all(&root).unwrap();
    let root = external_path(root.canonicalize().unwrap());
    let mut h = Harness { root, seq: 0 };
    let p = h.root.clone();
    for t in ["git", "cargo", "npm", "node", "uv"] {
        h.ok(&p, t, &["--version"]);
    }
    for k in ["cargo", "npm", "uv"] {
        scenario(&mut h, k);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_windows_paths() {
        assert_eq!(
            windows_path(r"\\?\C:\fixture dir\s6.exe"),
            "C:/fixture dir/s6.exe"
        );
        assert!(std::panic::catch_unwind(|| windows_path(r"\\?\UNC\server\share")).is_err());
    }
    fn e() -> Evidence {
        Evidence {
            tree: "tree".into(),
            parent: "parent".into(),
            policy: "policy".into(),
            attempt: 2,
            passed: true,
        }
    }
    #[test]
    fn exact_success() {
        assert!(accepts(&e(), "tree", "parent", "policy", 2));
    }
    #[test]
    fn reject_stale_dimensions() {
        for (t, p, c, a) in [
            ("old", "parent", "policy", 2),
            ("tree", "old", "policy", 2),
            ("tree", "parent", "old", 2),
            ("tree", "parent", "policy", 1),
        ] {
            assert!(!accepts(&e(), t, p, c, a));
        }
    }
    #[test]
    fn failed_gate_never_passes() {
        let mut x = e();
        x.passed = false;
        assert!(!accepts(&x, "tree", "parent", "policy", 2));
    }
    #[test]
    fn reject_executable_and_config_changes() {
        assert!(trusted(b"approved", b"approved"));
        assert!(!trusted(b"approved", b"exit 0"));
    }
    #[test]
    fn independent_manifest_inputs() {
        for k in ["cargo", "npm", "uv"] {
            assert_ne!(manifest(k, true, false), manifest(k, false, true));
            assert_ne!(manifest(k, false, false), manifest(k, true, true));
        }
    }
}
