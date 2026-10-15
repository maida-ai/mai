//! Standalone Git CLI qualification; no production library or dependencies.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};
type Snapshot = BTreeMap<String, (String, Vec<u8>)>;
struct Lab {
    repo: PathBuf,
    log: fs::File,
    serial: usize,
}
impl Lab {
    fn command(&mut self, args: &[&str], input: &[u8]) -> (i32, String) {
        writeln!(self.log, "git {:?} input_bytes={}", args, input.len()).unwrap();
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(&self.repo)
            .args([
                "-c",
                "user.name=S2 fixture",
                "-c",
                "user.email=s2@example.invalid",
                "-c",
                "core.autocrlf=false",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_AUTHOR_DATE", "2001-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2001-01-01T00:00:00Z")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let bytes = input.to_vec();
        let writer = std::thread::spawn(move || stdin.write_all(&bytes));
        let out = child.wait_with_output().unwrap();
        writer.join().unwrap().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let code = out.status.code().unwrap_or(-1);
        writeln!(
            self.log,
            "exit={code}\n{text}{}",
            String::from_utf8_lossy(&out.stderr)
        )
        .unwrap();
        (code, text.trim().into())
    }
    fn git(&mut self, args: &[&str], input: &[u8]) -> String {
        let (code, out) = self.command(args, input);
        assert_eq!(code, 0, "git {args:?}");
        out
    }
    fn tree(&mut self, s: &Snapshot) -> String {
        self.git(&["read-tree", "--empty"], b"");
        let mut index = String::new();
        let mut blobs = BTreeMap::new();
        for (path, (mode, bytes)) in s {
            let hash = blobs
                .entry(bytes.clone())
                .or_insert_with(|| self.git(&["hash-object", "-w", "--stdin"], bytes));
            index.push_str(&format!("{mode} {hash}\t{path}\n"));
        }
        self.git(&["update-index", "--index-info"], index.as_bytes());
        self.git(&["write-tree"], b"")
    }
    fn commit(&mut self, s: &Snapshot, parents: &[&str]) -> String {
        let tree = self.tree(s);
        self.commit_tree(&tree, parents)
    }
    fn commit_tree(&mut self, tree: &str, parents: &[&str]) -> String {
        let mut args = vec!["commit-tree", tree];
        for p in parents {
            args.extend(["-p", p]);
        }
        self.serial += 1;
        self.git(&args, format!("fixture {}\n", self.serial).as_bytes())
    }
    fn merge(&mut self, base: &str, left: &str, right: &str) -> (i32, String) {
        self.command(
            &[
                "merge-tree",
                "--write-tree",
                &format!("--merge-base={base}"),
                left,
                right,
            ],
            b"",
        )
    }
    fn differential(
        &mut self,
        name: &str,
        base: &str,
        left: &str,
        right: &str,
        clean: bool,
        expected: Option<&Snapshot>,
    ) {
        let (code, out) = self.merge(base, left, right);
        assert_eq!(code, if clean { 0 } else { 1 }, "{name}: {out}");
        if let Some(s) = expected {
            let tree = self.tree(s);
            assert_eq!(
                out.lines().next().unwrap(),
                tree,
                "{name}: independent expected tree"
            );
        }
        // Oracle checkout occurs in a separate repository. Integration itself is bare.
        let bare = self.repo.clone();
        let oracle = bare
            .parent()
            .unwrap()
            .join(format!("oracle-{}", self.serial));
        self.git(
            &[
                "clone",
                "--no-checkout",
                bare.to_str().unwrap(),
                oracle.to_str().unwrap(),
            ],
            b"",
        );
        self.repo = oracle;
        self.git(&["checkout", "--detach", left], b"");
        let (oracle_code, _) = self.command(&["cherry-pick", right], b"");
        assert_eq!(
            oracle_code == 0,
            clean,
            "{name}: cherry-pick classification"
        );
        if clean {
            let oracle_tree = self.git(&["rev-parse", "HEAD^{tree}"], b"");
            assert_eq!(
                out.lines().next().unwrap(),
                oracle_tree,
                "{name}: differential tree"
            );
        }
        if !clean {
            let oracle_stages = self.git(&["ls-files", "--unmerged"], b"");
            let stages = |text: &str| {
                let mut entries: Vec<String> = text
                    .lines()
                    .filter(|line| line.contains('\t'))
                    .map(|line| line.replace(&format!("~{left}"), "~HEAD"))
                    .collect();
                entries.sort();
                entries
            };
            assert!(!stages(&out).is_empty(), "{name}: missing conflict stages");
            assert_eq!(
                stages(&out),
                stages(&oracle_stages),
                "{name}: conflict paths, modes, stages and blobs"
            );
        }
        self.repo = bare;
        writeln!(
            self.log,
            "PASS case={name} clean={clean} base={base} left={left} right={right}"
        )
        .unwrap();
    }
}
fn put(s: &mut Snapshot, p: &str, v: &[u8]) {
    s.insert(p.into(), ("100644".into(), v.into()));
}
fn initial() -> Snapshot {
    let mut s = Snapshot::new();
    put(
        &mut s,
        "text",
        b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
    );
    put(&mut s, "binary", b"\0base");
    s
}
fn corpus(l: &mut Lab) {
    for name in [
        "disjoint",
        "same-hunk",
        "rename-edit",
        "delete-edit",
        "mode",
        "binary",
        "file-directory",
        "stale",
        "attributes",
    ] {
        let b = initial();
        let base = l.commit(&b, &[]);
        let mut a = b.clone();
        let mut c = b.clone();
        let mut expected = None;
        match name {
            "disjoint" | "stale" => {
                put(
                    &mut a,
                    "text",
                    b"ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
                );
                put(
                    &mut c,
                    "text",
                    b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nNINE\n",
                );
                let mut e = a.clone();
                put(
                    &mut e,
                    "text",
                    b"ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nNINE\n",
                );
                expected = Some(e);
            }
            "same-hunk" => {
                put(&mut a, "text", b"left\n");
                put(&mut c, "text", b"right\n");
            }
            "rename-edit" => {
                let value = a.remove("text").unwrap();
                a.insert("renamed".into(), value);
                put(
                    &mut c,
                    "text",
                    b"ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
                );
                let mut e = a.clone();
                e.insert("renamed".into(), c["text"].clone());
                expected = Some(e);
            }
            "delete-edit" => {
                a.remove("text");
                put(&mut c, "text", b"edited\n");
            }
            "mode" => {
                a.get_mut("text").unwrap().0 = "100755".into();
                put(&mut c, "text", b"edited\n");
                let mut e = c.clone();
                e.get_mut("text").unwrap().0 = "100755".into();
                expected = Some(e);
            }
            "binary" => {
                put(&mut a, "binary", b"\0left");
                put(&mut c, "binary", b"\0right");
            }
            "file-directory" => {
                put(&mut a, "path", b"file");
                put(&mut c, "path/child", b"child");
            }
            "attributes" => {
                put(&mut a, ".gitattributes", b"text -merge\n");
                put(&mut a, "text", b"left\n");
                put(&mut c, "text", b"right\n");
            }
            _ => unreachable!(),
        }
        let mut left = l.commit(&a, &[&base]);
        if name == "stale" {
            left = l.commit(&a, &[&left]);
        }
        let right = l.commit(&c, &[&base]);
        l.differential(
            name,
            &base,
            &left,
            &right,
            expected.is_some(),
            expected.as_ref(),
        );
    }
    // Parent was squash-integrated, then independently amended on trunk. Child must apply only its own delta.
    let b = initial();
    let root = l.commit(&b, &[]);
    let mut parent = b.clone();
    put(&mut parent, "parent", b"original");
    let submitted = l.commit(&parent, &[&root]);
    let squash = l.commit(&parent, &[&root]);
    let mut trunk = parent.clone();
    put(&mut trunk, "parent", b"new trunk value");
    let trunk_id = l.commit(&trunk, &[&squash]);
    let mut child = parent.clone();
    put(&mut child, "child", b"delta");
    let child_id = l.commit(&child, &[&submitted]);
    let mut expected = trunk.clone();
    put(&mut expected, "child", b"delta");
    l.differential(
        "dependent-squash",
        &submitted,
        &trunk_id,
        &child_id,
        true,
        Some(&expected),
    );
    let (code, _) = l.merge(&root, &trunk_id, &child_id);
    assert_eq!(code, 1, "naive base must expose parent replay conflict");
    let unrelated = l.commit(&b, &[]);
    assert_eq!(
        l.command(&["merge-base", "--all", &root, &unrelated], b"")
            .0,
        1
    );
    let a = l.commit(&b, &[&root]);
    let c = l.commit(&b, &[&root]);
    let x = l.commit(&b, &[&a, &c]);
    let y = l.commit(&b, &[&c, &a]);
    let bases = l.git(&["merge-base", "--all", &x, &y], b"");
    assert_eq!(bases.lines().count(), 2);
    assert!(select_base(&bases).is_err());
    assert!(select_base("").is_err());
    writeln!(
        l.log,
        "PASS missing/multiple bases rejected; explicit mapped dependent base required"
    )
    .unwrap();
}
fn select_base(s: &str) -> Result<&str, &str> {
    let v: Vec<_> = s.lines().collect();
    if v.len() == 1 {
        Ok(v[0])
    } else {
        Err("require unique base or explicit verified dependency mapping")
    }
}
fn benchmark(l: &mut Lab) {
    let mut s = Snapshot::new();
    for i in 0..100_000 {
        put(
            &mut s,
            &format!("files/{:03}/{i:06}.txt", i / 1000),
            b"base\n",
        );
    }
    let base = l.commit(&s, &[]);
    let count = l
        .git(&["ls-tree", "-r", "--name-only", &base], b"")
        .lines()
        .count();
    assert_eq!(count, 100_000);
    let mut a = s.clone();
    put(&mut a, "files/000/000000.txt", b"left\n");
    let left = l.commit(&a, &[&base]);
    put(&mut s, "files/099/099999.txt", b"right\n");
    let right = l.commit(&s, &[&base]);
    let mut times = Vec::new();
    for i in 0..31 {
        let start = Instant::now();
        let (code, _) = l.merge(&base, &left, &right);
        let ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(code, 0);
        times.push(ms);
        writeln!(
            l.log,
            "TIMING sample={i} milliseconds={ms:.6} tracked_files={count}"
        )
        .unwrap();
    }
    let median = median(&times);
    writeln!(
        l.log,
        "CRITERION median_ms={median:.6} threshold_ms=100 strict_less=true pass={}",
        median < 100.
    )
    .unwrap();
    assert!(
        median < 100.,
        "S2 NO-GO: median {median} ms is not below 100 ms; ADR review required"
    );
}
fn median(v: &[f64]) -> f64 {
    assert!(!v.is_empty());
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "run <new-root>");
    assert!(["run", "scenarios"].contains(&args[1].as_str()));
    let root = Path::new(&args[2]);
    assert!(!root.exists());
    fs::create_dir_all(root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let repo = root.join("authority.git");
    fs::create_dir(&repo).unwrap();
    let mut l = Lab {
        repo,
        log: fs::File::create(root.join("scenarios.log")).unwrap(),
        serial: 0,
    };
    l.git(&["init", "--bare"], b"");
    corpus(&mut l);
    trust(&mut l);
    manifests(&mut l);
    if args[1] == "run" {
        benchmark(&mut l);
    }
    println!(
        "PASS S2 Linux/native-host scenarios; inspect scenarios.log for exact evidence and timings"
    );
}
mod manifest;
use manifest::manifests;
fn trust(l: &mut Lab) {
    let mut b = initial();
    put(&mut b, ".gitattributes", b"text merge=fixture\n");
    let base = l.commit(&b, &[]);
    let mut a = b.clone();
    put(&mut a, "text", b"left\n");
    let left = l.commit(&a, &[&base]);
    put(&mut b, "text", b"right\n");
    put(
        &mut b,
        "candidate-controlled-driver.sh",
        b"printf malicious > \"$1\"\n",
    );
    let right = l.commit(&b, &[&base]);
    // info/attributes represents approved policy, outside all candidate trees.
    fs::write(l.repo.join("info/attributes"), "text merge=fixture\n").unwrap();
    l.git(
        &["config", "merge.fixture.driver", "printf trusted > %A"],
        b"",
    );
    let (code, out) = l.merge(&base, &left, &right);
    assert_eq!(code, 0);
    let tree = out.lines().next().unwrap();
    assert_eq!(l.git(&["show", &format!("{tree}:text")], b""), "trusted");
    // A driver command pointing into a checkout cannot be considered pinned.
    l.git(
        &[
            "config",
            "merge.fixture.driver",
            "sh candidate-controlled-driver.sh %A",
        ],
        b"",
    );
    let (code, _) = l.merge(&base, &left, &right);
    assert_ne!(code, 0);
    fs::write(l.repo.join("info/attributes"), "text -merge\n").unwrap();
    l.git(&["config", "--unset", "merge.fixture.driver"], b"");
    assert_eq!(l.merge(&base, &left, &right).0, 1);
    writeln!(l.log,"PASS bare merge executes trusted configured driver; candidate-tracked script is not checked out and fails; approved info/attributes binary policy conflicts. This is not an arbitrary-driver sandbox.").unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique_base() {
        assert_eq!(select_base("abc"), Ok("abc"));
        assert!(select_base("").is_err());
        assert!(select_base("abc\ndef").is_err());
    }
    #[test]
    fn timing_threshold() {
        assert_eq!(median(&[101., 99., 100.]), 100.);
        assert!(!(median(&[100.]) < 100.));
    }
    #[test]
    #[should_panic]
    fn empty_samples_fail() {
        median(&[]);
    }
}
