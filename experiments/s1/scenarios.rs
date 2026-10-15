use super::*;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::time::{Duration, Instant};
struct Fixture {
    root: PathBuf,
    authority: PathBuf,
    alice: PathBuf,
    bob: PathBuf,
    url: String,
    log: fs::File,
    checks: usize,
    container_image: Option<String>,
}
impl Fixture {
    fn new(root: PathBuf, http: Option<&str>, fallback: bool) -> Self {
        fs::create_dir(&root).unwrap();
        let authority = root.join("authority.git");
        let alice = root.join("alice");
        let bob = root.join("bob");
        text(
            command("git")
                .args(["init", "--bare", "--initial-branch=main", "--template="])
                .arg(git_path(&authority))
                .output()
                .unwrap(),
        );
        text(
            command("git")
                .args(["init", "--initial-branch=main", "--template="])
                .arg(git_path(&alice))
                .output()
                .unwrap(),
        );
        fs::write(alice.join("base"), "baseline\n").unwrap();
        g(&alice, &["add", "."]);
        g(&alice, &["commit", "-m", "baseline"]);
        // Bootstrap only, before receive policy is installed.
        g(
            &alice,
            &["push", &git_path(&authority), "HEAD:refs/heads/main"],
        );
        text(
            command("git")
                .arg("clone")
                .arg("--template=")
                .arg(git_path(&authority))
                .arg(git_path(&bob))
                .output()
                .unwrap(),
        );
        g(
            &authority,
            &["config", "receive.advertisePushOptions", "true"],
        );
        g(&authority, &["config", "http.receivepack", "true"]);
        g(&authority, &["config", "receive.advertiseAtomic", "false"]);
        g(&authority, &["config", "core.fsync", "committed"]);
        if fallback {
            fs::write(authority.join("fallback"), "").unwrap();
        } else {
            g(&authority, &["config", "receive.procReceiveRefs", "refs/"]);
        }
        fs::create_dir_all(authority.join("hooks")).unwrap();
        let binary = env::current_exe().unwrap();
        for (hook, mode) in [
            ("pre-receive", "pre"),
            ("proc-receive", "proc"),
            ("post-receive", "post"),
        ] {
            if hook == "post-receive" && !fallback {
                continue;
            }
            let p = authority.join("hooks").join(hook);
            let quoted = git_path(&binary).replace('\'', "'\"'\"'");
            fs::write(&p, format!("#!/bin/sh\nexec '{quoted}' {mode}\n")).unwrap();
            #[cfg(unix)]
            fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let url = http
            .map(|h| {
                format!(
                    "{h}/{}/authority.git",
                    root.file_name().unwrap().to_str().unwrap()
                )
            })
            .unwrap_or_else(|| file_url(&git_path(&authority)));
        let log = fs::File::create(root.join("pushes.log")).unwrap();
        let container_image = if root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("container-")
        {
            Some(env::var("S1_CONTAINER_IMAGE").expect("container image required"))
        } else {
            None
        };
        let mut fixture = Self {
            root,
            authority,
            alice,
            bob,
            url,
            log,
            checks: 0,
            container_image,
        };
        if fixture.container_image.is_some() {
            fixture.url = fixture.url.replace("127.0.0.1", "host.docker.internal");
            let client = fixture.root.join("container-alice");
            fs::create_dir(&client).unwrap();
            let out = fixture
                .client_command(&client, true)
                .args(["clone", "--template=", &fixture.url, "."])
                .output()
                .unwrap();
            writeln!(
                fixture.log,
                "container clone exit={}\n{}{}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
            .unwrap();
            fixture.check("container cloned authority over HTTP", out.status.success());
            fixture.alice = client;
        }
        fixture
    }
    fn check(&mut self, label: &str, value: bool) {
        assert!(value, "FAIL {label} in {}", self.root.display());
        self.checks += 1;
        println!(
            "PASS {}: {label}",
            self.root.file_name().unwrap().to_str().unwrap()
        );
    }
    fn commit(&self, file: &str) -> String {
        fs::write(self.alice.join(file), format!("{file}\n")).unwrap();
        g(&self.alice, &["add", file]);
        g(&self.alice, &["commit", "-m", file]);
        g(&self.alice, &["rev-parse", "HEAD"])
    }
    fn client_command(&self, client: &Path, authenticated: bool) -> Command {
        let mut c = if let Some(image) = &self.container_image {
            container_command(client, image)
        } else {
            let mut c = command("git");
            c.arg("-C").arg(git_path(client));
            c
        };
        c.args(["-c", "http.lowSpeedLimit=1", "-c", "http.lowSpeedTime=15"]);
        if authenticated && self.url.starts_with("http:") {
            c.args([
                "-c",
                "http.extraHeader=Authorization: Bearer s1-fixture-alice",
                "-c",
                "http.postBuffer=2097152",
            ]);
        }
        c
    }
    fn push_command(&self, client: &Path, specs: &[&str], options: &[&str]) -> Command {
        let mut c = self.client_command(client, true);
        c.args(["push", "--porcelain"]);
        for opt in options {
            c.args(["-o", opt]);
        }
        c.arg(&self.url).args(specs);
        c
    }
    fn push(&mut self, specs: &[&str], options: &[&str], success: bool, reason: &str) {
        let now = Instant::now();
        let out = self
            .push_command(&self.alice, specs, options)
            .output()
            .unwrap();
        let detail = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        writeln!(
            self.log,
            "refs={specs:?} options={options:?} exit={} elapsed_ms={}\n{detail}",
            out.status,
            now.elapsed().as_millis()
        )
        .unwrap();
        self.check(
            reason,
            out.status.success() == success && (success || detail.contains(reason)),
        );
    }
    fn retained(&self, oid: &str) -> bool {
        g(
            &self.authority,
            &[
                "for-each-ref",
                "--format=%(objectname)",
                &format!("refs/mai/changes/{oid}"),
            ],
        ) == oid
    }
    fn queue_count(&self) -> usize {
        fs::read_dir(self.authority.join("queue")).unwrap().count()
    }
    fn fault(&self, name: &str, on: bool) {
        let p = self.authority.join(name);
        if on {
            fs::write(p, "fixture fault\n").unwrap();
        } else {
            fs::remove_file(p).unwrap();
        }
    }
}
fn proc_suite(root: PathBuf, http: Option<&str>) -> usize {
    let mut f = Fixture::new(root, http, false);
    let base = g(&f.authority, &["rev-parse", "main"]);
    let first = f.commit("first");
    f.push(
        &["HEAD:refs/for/main"],
        &["mai.wait=true", "mai.priority=low", "mai.touches=first"],
        true,
        "registered without validator/wait process",
    );
    f.check(
        "reference and queued receipt exist at successful push return",
        f.retained(&first) && f.authority.join("queue").join(&first).exists(),
    );
    let events = fs::read_to_string(f.authority.join("s1-audit.log")).unwrap();
    let at = |s: &str| events.find(&format!("{s} {first}")).unwrap();
    f.check(
        "separate engine visibility, retention and queue precede hook ok",
        at("engine-visible") < at("engine-retained")
            && at("engine-retained") < at("engine-queued")
            && at("engine-queued") < at("hook-ok"),
    );
    f.check(
        "fresh pre-receive quarantine is not visible to separate engine",
        events.contains(&format!(
            "pre-visible inherited=true separate=false {first}"
        )),
    );
    f.check(
        "submission ref is virtual and trunk has not moved",
        g(&f.authority, &["for-each-ref", "refs/for/"]).is_empty()
            && g(&f.authority, &["rev-parse", "main"]) == base,
    );
    f.push(&["HEAD:refs/for/main"], &[], true, "identical retry");
    f.check("retry creates one queue entry", f.queue_count() == 1);
    f.push(&["HEAD:refs/drafts/alice"], &[], true, "owned draft");
    f.push(&[":refs/drafts/alice"], &[], false, "DELETE_FORBIDDEN");
    for r in [
        "refs/heads/main",
        "refs/mai/candidates/evil",
        "refs/notes/mai",
        "refs/drafts/bob",
        "refs/for/other",
    ] {
        f.push(&[&format!("HEAD:{r}")], &[], false, "REF_FORBIDDEN");
    }
    for (opts, reason) in [
        (vec!["mai.priority=high"], "GRANT_REQUIRED"),
        (vec!["mai.priority=sev"], "GRANT_REQUIRED"),
        (vec!["mai.priority=bogus"], "INVALID_PRIORITY"),
        (vec!["mai.fixes=expired"], "INACTIVE_INCIDENT"),
        (vec!["mai.grant=sev"], "UNKNOWN_OPTION"),
        (vec!["mai.touches=a,,b"], "INVALID_TOUCHES"),
        (vec!["mai.wait=true", "mai.wait=false"], "DUPLICATE_OPTION"),
        (vec!["mai.wait=true"; 17], "OPTION_LIMIT"),
    ] {
        f.push(&["HEAD:refs/for/main"], &opts, false, reason);
    }
    let huge = format!("mai.touches={}", "x".repeat(1024));
    f.push(&["HEAD:refs/for/main"], &[&huge], false, "OPTION_LIMIT");
    if http.is_some() {
        f.push(
            &["HEAD:refs/drafts/bob"],
            &["mai.agent=bob"],
            false,
            "IDENTITY_MISMATCH",
        );
        f.push(
            &["HEAD:refs/for/main"],
            &["mai.priority=sev", "mai.fixes=incident-1"],
            true,
            "synthetic authenticated scoped incident grant",
        );
        let unauthorized = f
            .client_command(&f.bob, false)
            .args(["push", &f.url, "HEAD:refs/for/main"])
            .output()
            .unwrap();
        f.check(
            "HTTP without fixture credential denied",
            !unauthorized.status.success()
                && String::from_utf8_lossy(&unauthorized.stderr).contains("403"),
        );
    } else {
        f.push(
            &["HEAD:refs/drafts/bob"],
            &["mai.agent=bob"],
            true,
            "declared file identity can be spoofed (not authentication)",
        );
        f.push(
            &["HEAD:refs/for/main"],
            &["mai.priority=sev", "mai.fixes=incident-1"],
            false,
            "GRANT_REQUIRED",
        );
    }
    let second = f.commit("second");
    f.push(
        &["HEAD:refs/for/main", "HEAD:refs/heads/main"],
        &[],
        false,
        "REF_FORBIDDEN",
    );
    f.check(
        "mixed push retains admitted ref while rejecting trunk",
        f.retained(&second) && g(&f.authority, &["rev-parse", "main"]) == base,
    );
    let third = f.commit("third");
    f.fault("engine-down", true);
    f.push(&["HEAD:refs/for/main"], &[], false, "ENGINE_UNAVAILABLE");
    f.check(
        "engine down yields no admission",
        !f.retained(&third) && !f.authority.join("queue").join(&third).exists(),
    );
    f.fault("engine-down", false);
    f.push(
        &["HEAD:refs/for/main"],
        &[],
        true,
        "retry after engine recovery",
    );
    let fourth = f.commit("fourth");
    f.fault("crash-after-ref", true);
    f.push(&["HEAD:refs/for/main"], &[], false, "INTERRUPTED_AFTER_REF");
    f.check(
        "interrupted registration retains object without queue claim",
        f.retained(&fourth) && !f.authority.join("queue").join(&fourth).exists(),
    );
    f.fault("crash-after-ref", false);
    f.push(
        &["HEAD:refs/for/main"],
        &[],
        true,
        "retry reconciles retained object",
    );
    let fifth = f.commit("fifth");
    f.fault("lose-reply", true);
    let before = f.queue_count();
    let out = f
        .push_command(&f.alice, &["HEAD:refs/for/main"], &[])
        .output()
        .unwrap();
    writeln!(
        f.log,
        "lost-reply exit={}\n{}{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .unwrap();
    f.check(
        "hook exits after registration before reply",
        !out.status.success() && f.retained(&fifth) && f.queue_count() == before + 1,
    );
    f.fault("lose-reply", false);
    f.push(&["HEAD:refs/for/main"], &[], true, "retry after lost reply");
    f.check(
        "lost reply retry does not duplicate registration",
        f.queue_count() == before + 1,
    );
    // Never exercise customer refs; all destructive fault/replay operations are fixture-local.
    if http.is_none() {
        let sixth = f.commit("sixth");
        f.fault("pause-reply", true);
        let mut child = f
            .push_command(&f.alice, &["HEAD:refs/for/main"], &[])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !f.authority.join("reply-ready").exists() {
            assert!(Instant::now() < deadline, "disconnect rendezvous timeout");
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        f.fault("release-reply", true);
        f.fault("pause-reply", false);
        f.check(
            "killed client leaves acknowledged-side registration recoverable",
            f.retained(&sixth) && f.authority.join("queue").join(&sixth).exists(),
        );
        let count = f.queue_count();
        f.push(
            &["HEAD:refs/for/main"],
            &[],
            true,
            "retry after client disconnect",
        );
        f.check("disconnect retry is idempotent", f.queue_count() == count);
    }
    f.check(
        "no validation or landing fabricated",
        g(&f.authority, &["rev-parse", "main"]) == base
            && fs::read_dir(f.authority.join("queue"))
                .unwrap()
                .all(|p| fs::read_to_string(p.unwrap().path()).unwrap() == "queued\n"),
    );
    f.checks
}
fn fallback_suite(root: PathBuf, http: Option<&str>) -> usize {
    let mut f = Fixture::new(root, http, true);
    let oid = f.commit("incoming");
    let incoming = format!("refs/incoming/alice/{oid}");
    let spec = format!("HEAD:{incoming}");
    f.fault("engine-down", true);
    let out = f.push_command(&f.alice, &[&spec], &[]).output().unwrap();
    let detail = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    writeln!(f.log, "fallback engine down exit={}\n{detail}", out.status).unwrap();
    f.check(
        "fallback transport success is not queued admission",
        out.status.success()
            && detail.contains("RETAINED_PENDING_REGISTRATION")
            && !f.retained(&oid),
    );
    f.check(
        "incoming ref retains unregistered commit",
        g(&f.authority, &["rev-parse", &incoming]) == oid,
    );
    f.fault("engine-down", false);
    f.push(
        &[&spec],
        &[],
        true,
        "identical fallback retry is up-to-date",
    );
    f.check(
        "up-to-date retry does not rerun post-receive",
        !f.retained(&oid),
    );
    // Explicit startup reconciliation, not an automatic retry claim.
    let refs = g(
        &f.authority,
        &["for-each-ref", "--format=%(objectname)", "refs/incoming/"],
    );
    for oid in refs.lines() {
        register(&f.authority, oid).unwrap();
    }
    f.check(
        "reconciliation registers retained incoming exactly once",
        f.retained(&oid) && f.queue_count() == 1,
    );
    register(&f.authority, &oid).unwrap();
    f.check("repeated reconciliation idempotent", f.queue_count() == 1);
    f.push(&["HEAD:refs/heads/main"], &[], false, "REF_FORBIDDEN");
    f.push(&["HEAD:refs/for/main"], &[], false, "REF_FORBIDDEN");
    f.push(
        &[&format!("HEAD:refs/incoming/bob/{oid}")],
        &[],
        false,
        "REF_FORBIDDEN",
    );
    let next = f.commit("incoming-next");
    f.push(
        &[&format!("HEAD:refs/incoming/alice/{next}")],
        &["mai.priority=sev"],
        false,
        "GRANT_REQUIRED",
    );
    f.push(
        &[&format!("HEAD:refs/incoming/alice/{next}")],
        &[],
        true,
        "fallback healthy post-receive admission",
    );
    f.check(
        "fallback healthy commit is retained and queued",
        f.retained(&next) && f.queue_count() == 2,
    );
    f.checks
}
fn safe_sync(
    client: &Path,
    submitted: &str,
    landed: &str,
    outcome: &str,
) -> Result<(), &'static str> {
    if outcome != "landed" {
        return Err("PARENT_NOT_LANDED");
    }
    if !g(
        client,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignored",
        ],
    )
    .is_empty()
    {
        return Err("DIRTY_OR_UNTRACKED");
    }
    if !git(client, &["merge-base", "--is-ancestor", submitted, "HEAD"])
        .status
        .success()
    {
        return Err("SUBMITTED_NOT_ANCESTOR");
    }
    if !git(client, &["rebase", "--onto", landed, submitted])
        .status
        .success()
    {
        return Err("SYNC_CONFLICT");
    }
    Ok(())
}
fn sync_suite(root: PathBuf) -> usize {
    let mut f = Fixture::new(root, None, false);
    let base = g(&f.authority, &["rev-parse", "main"]);
    let a = f.commit("parent-a");
    f.push(&["HEAD:refs/for/main"], &[], true, "parent submitted");
    let b = f.commit("child-b");
    f.push(
        &["HEAD:refs/for/main"],
        &[],
        true,
        "dependent child submitted before parent lands",
    );
    let c = f.commit("later-c");
    f.check(
        "pending parent blocks sync without changing local tip",
        safe_sync(&f.alice, &a, &base, "queued") == Err("PARENT_NOT_LANDED")
            && g(&f.alice, &["rev-parse", "HEAD"]) == c,
    );
    f.check(
        "rejected parent preserves descendant work",
        safe_sync(&f.alice, &a, &base, "rejected") == Err("PARENT_NOT_LANDED")
            && fs::read_to_string(f.alice.join("child-b")).unwrap() == "child-b\n",
    );
    // Simulated accepted parent, not a gate or integration-engine qualification.
    let tree = g(&f.authority, &["rev-parse", &format!("{a}^{{tree}}")]);
    let landed = g(
        &f.authority,
        &[
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-m",
            "fixture squash parent\n\nMai-Revision: 1",
        ],
    );
    g(
        &f.authority,
        &["update-ref", "refs/heads/main", &landed, &base],
    );
    g(&f.alice, &["fetch", &git_path(&f.authority), "main"]);
    fs::write(f.alice.join("base"), "dirty work\n").unwrap();
    f.check(
        "dirty sync refuses without losing work",
        safe_sync(&f.alice, &a, &landed, "landed") == Err("DIRTY_OR_UNTRACKED")
            && fs::read_to_string(f.alice.join("base")).unwrap() == "dirty work\n",
    );
    fs::write(f.alice.join("base"), "baseline\n").unwrap();
    fs::write(f.alice.join("untracked"), "keep\n").unwrap();
    f.check(
        "untracked sync refuses without losing work",
        safe_sync(&f.alice, &a, &landed, "landed") == Err("DIRTY_OR_UNTRACKED")
            && fs::read_to_string(f.alice.join("untracked")).unwrap() == "keep\n",
    );
    fs::rename(
        f.alice.join("untracked"),
        f.root.join("preserved-untracked"),
    )
    .unwrap();
    fs::create_dir_all(f.alice.join(".git/info")).unwrap();
    fs::write(f.alice.join(".git/info/exclude"), "ignored-local\n").unwrap();
    fs::write(f.alice.join("ignored-local"), "keep ignored work\n").unwrap();
    f.check(
        "ignored local files also block sync without data loss",
        safe_sync(&f.alice, &a, &landed, "landed") == Err("DIRTY_OR_UNTRACKED")
            && fs::read_to_string(f.alice.join("ignored-local")).unwrap() == "keep ignored work\n",
    );
    fs::rename(
        f.alice.join("ignored-local"),
        f.root.join("preserved-ignored"),
    )
    .unwrap();
    f.check(
        "squash sync replays subsequent work",
        safe_sync(&f.alice, &a, &landed, "landed").is_ok(),
    );
    f.check(
        "exactly child and later commit replayed",
        g(
            &f.alice,
            &["rev-list", "--count", &format!("{landed}..HEAD")],
        ) == "2",
    );
    f.check(
        "sync preserves final tree and does not replay accepted parent",
        g(&f.alice, &["rev-parse", "HEAD^{tree}"])
            == g(&f.alice, &["rev-parse", &format!("{c}^{{tree}}")])
            && g(
                &f.alice,
                &["log", "--format=%s", &format!("{landed}..HEAD")],
            ) == "later-c\nchild-b",
    );
    f.check(
        "original submitted dependency remains retained",
        f.retained(&a) && f.retained(&b),
    );
    let rewritten_child = g(&f.alice, &["rev-parse", "HEAD~"]);
    let child_tree = g(&f.authority, &["rev-parse", &format!("{b}^{{tree}}")]);
    let child_landed = g(
        &f.authority,
        &[
            "commit-tree",
            &child_tree,
            "-p",
            &landed,
            "-m",
            "fixture squash child\n\nMai-Revision: 2",
        ],
    );
    g(
        &f.authority,
        &["update-ref", "refs/heads/main", &child_landed, &landed],
    );
    g(&f.alice, &["fetch", &git_path(&f.authority), "main"]);
    let before = g(&f.alice, &["rev-parse", "HEAD"]);
    f.check(
        "original submitted child is no longer a valid local replay boundary",
        safe_sync(&f.alice, &b, &child_landed, "landed") == Err("SUBMITTED_NOT_ANCESTOR")
            && g(&f.alice, &["rev-parse", "HEAD"]) == before,
    );
    f.check(
        "mapped rewritten child permits second squash sync",
        safe_sync(&f.alice, &rewritten_child, &child_landed, "landed").is_ok()
            && g(
                &f.alice,
                &["rev-list", "--count", &format!("{child_landed}..HEAD")],
            ) == "1"
            && g(&f.alice, &["rev-parse", "HEAD^{tree}"])
                == g(&f.alice, &["rev-parse", &format!("{c}^{{tree}}")]),
    );
    f.checks
}
pub fn run(root: &Path) {
    assert!(
        !root.exists(),
        "output directory must not exist (no reuse or cleanup)"
    );
    fs::create_dir_all(root).unwrap();
    let root = root.canonicalize().unwrap();
    println!("S1 fixture run: {}", root.display());
    println!("Native platform: {}/{}", env::consts::OS, env::consts::ARCH);
    for (p, args) in [("git", vec!["--version"]), ("rustc", vec!["--version"])] {
        println!("{}", text(command(p).args(args).output().unwrap()));
    }
    let mut count = proc_suite(root.join("file-proc"), None);
    count += fallback_suite(root.join("file-fallback"), None);
    count += sync_suite(root.join("sync"));
    let server = transport::Server::start(root.clone());
    count += proc_suite(root.join("http-proc"), Some(&server.url));
    count += fallback_suite(root.join("http-fallback"), Some(&server.url));
    if let Ok(image) = env::var("S1_CONTAINER_IMAGE") {
        assert!(
            image.contains("@sha256:"),
            "pin the container image by digest"
        );
        println!("Container image: {image}");
        println!(
            "Docker server: {}",
            text(
                command("docker")
                    .args(["version", "--format", "{{.Server.Version}}"])
                    .output()
                    .unwrap()
            )
        );
        println!(
            "Container client: {}",
            text(
                command("docker")
                    .args([
                        "run",
                        "--rm",
                        "--pull=never",
                        "--entrypoint=git",
                        &image,
                        "--version"
                    ])
                    .output()
                    .unwrap()
            )
        );
        count += proc_suite(root.join("container-proc"), Some(&server.url));
        count += fallback_suite(root.join("container-fallback"), Some(&server.url));
        println!(
            "Container clone/push policy and recovery scenarios passed (Docker Desktop host.docker.internal)."
        );
    } else {
        println!(
            "PENDING: container transport (set S1_CONTAINER_IMAGE to an already-pulled image digest)."
        );
    }
    drop(server);
    println!(
        "PASS: {count} assertions on this runner. Other platforms, Git 2.40 minimum and production durability/IPC/grant lifecycle require separate evidence; S1 NOT QUALIFIED."
    );
}

#[cfg(unix)]
fn container_command(client: &Path, image: &str) -> Command {
    let owner = fs::metadata(client).unwrap();
    let mut c = command("docker");
    c.args([
        "run",
        "--rm",
        "--pull=never",
        "--entrypoint=git",
        "--workdir=/work",
        "--user",
        &format!("{}:{}", owner.uid(), owner.gid()),
        "--mount",
        &format!("type=bind,source={},target=/work", client.display()),
        "-e",
        "GIT_CONFIG_NOSYSTEM=1",
        "-e",
        "GIT_CONFIG_GLOBAL=/dev/null",
        "-e",
        "GIT_TERMINAL_PROMPT=0",
        image,
    ]);
    c
}

#[cfg(not(unix))]
fn container_command(_client: &Path, _image: &str) -> Command {
    panic!("container mode currently requires a Unix Docker Desktop host");
}
