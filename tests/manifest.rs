use mai::manifest::{FileManifest, FileUnit, Operation, compute};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn git(repo: &Path, args: &[&str], input: &[u8]) -> String {
    let mut child = Command::new("git")
        .arg("--git-dir")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "--bare"], b"");
    temp
}
fn blob(repo: &Path, bytes: &[u8]) -> String {
    git(repo, &["hash-object", "-w", "--stdin"], bytes)
}
fn tree(repo: &Path, entries: &[(&[u8], &str, &str)]) -> String {
    let mut input = Vec::new();
    for (path, mode, oid) in entries {
        let kind = match *mode {
            "160000" => "commit",
            "040000" => "tree",
            _ => "blob",
        };
        input.extend(format!("{mode} {kind} {oid}\t").bytes());
        input.extend(*path);
        input.push(0);
    }
    git(repo, &["mktree", "-z", "--missing"], &input)
}

#[test]
fn canonical_units_round_trip_raw_bytes_and_reject_aliases() {
    for path in [b"a b".as_slice(), b"%20", b"x:,;\r\n\t\\\xff", b"dir/a-_.~"] {
        let unit = FileUnit::from_git_path(path).unwrap();
        let json = serde_json::to_string(&unit).unwrap();
        assert_eq!(
            serde_json::from_str::<FileUnit>(&json)
                .unwrap()
                .as_git_path(),
            path
        );
    }
    assert_eq!(
        FileUnit::from_git_path(b"%20").unwrap().canonical(),
        "file:%2520"
    );
    assert_eq!(
        FileUnit::from_git_path(b"a b").unwrap().canonical(),
        "file:a%20b"
    );
    for invalid in [
        "file:",
        "file:/a",
        "file:a/",
        "file:a//b",
        "file:.",
        "file:a/../b",
        "file:a/./b",
        "file:%00",
        "file:%",
        "file:%2",
        "file:%ff",
        "file:%41",
        "file:a b",
        "file:a:b",
        "file:é",
        "file:a%2Fb",
        "object:a",
    ] {
        assert!(FileUnit::try_from(invalid.to_owned()).is_err(), "{invalid}");
    }
}

#[test]
fn actual_trees_report_rename_mode_binary_type_and_submodule_changes() {
    let temp = repo();
    let repo = temp.path();
    let a = blob(repo, b"\0original\xff");
    let b = blob(repo, b"\0changed\xfe");
    let commit_a = "1111111111111111111111111111111111111111";
    let commit_b = "2222222222222222222222222222222222222222";
    let base = tree(
        repo,
        &[
            (b"old", "100644", &a),
            (b"mode", "100644", &a),
            (b"binary", "100644", &a),
            (b"type", "100644", &a),
            (b"submodule", "160000", commit_a),
        ],
    );
    let result = tree(
        repo,
        &[
            (b"new", "100644", &a),
            (b"mode", "100755", &a),
            (b"binary", "100644", &b),
            (b"type", "120000", &a),
            (b"submodule", "160000", commit_b),
        ],
    );
    // Local config must not hide gitlink changes or collapse identical renames.
    git(repo, &["config", "diff.renames", "true"], b"");
    git(repo, &["config", "diff.ignoreSubmodules", "all"], b"");
    let manifest = compute(repo, &base, &result).unwrap();
    let entries: Vec<_> = manifest
        .entries
        .iter()
        .map(|e| (e.unit.canonical(), e.operation))
        .collect();
    assert_eq!(
        entries,
        vec![
            ("file:binary".into(), Operation::Modified),
            ("file:mode".into(), Operation::Modified),
            ("file:new".into(), Operation::Added),
            ("file:old".into(), Operation::Deleted),
            ("file:submodule".into(), Operation::Modified),
            ("file:type".into(), Operation::Modified)
        ]
    );
    assert_eq!(manifest, compute(repo, &base, &result).unwrap());
    let json = serde_json::to_value(&manifest).unwrap();
    assert_eq!(json["entries"][0]["fallback_reason"], "mvp-file-only");
    assert_eq!(json["entries"][0]["granularity"], "file");
    assert_eq!(
        serde_json::from_value::<FileManifest>(json).unwrap(),
        manifest
    );
}

#[test]
fn raw_sorting_unusual_names_and_trailer_boundaries() {
    let temp = repo();
    let repo = temp.path();
    let blob = blob(repo, b"data");
    let empty = tree(repo, &[]);
    let paths: Vec<Vec<u8>> = vec![
        b"z".to_vec(),
        b"\xff".to_vec(),
        b"%".to_vec(),
        b"a\nMai-Objects: forged".to_vec(),
        b"a\t\r,;\\".to_vec(),
    ];
    let entries: Vec<_> = paths
        .iter()
        .map(|p| (p.as_slice(), "100644", blob.as_str()))
        .collect();
    let unusual = tree(repo, &entries);
    let manifest = compute(repo, &empty, &unusual).unwrap();
    let mut sorted = paths;
    sorted.sort();
    assert_eq!(
        manifest
            .entries
            .iter()
            .map(|e| e.unit.as_git_path().to_vec())
            .collect::<Vec<_>>(),
        sorted
    );
    assert!(
        !manifest
            .trailer_value()
            .unwrap()
            .contains(['\r', '\n', '\t', ';'])
    );
    assert_eq!(
        compute(repo, &empty, &empty)
            .unwrap()
            .trailer_value()
            .unwrap(),
        "none"
    );
    for count in [20, 21] {
        let names: Vec<_> = (0..count).map(|n| format!("file{n:02}")).collect();
        let entries: Vec<_> = names
            .iter()
            .map(|n| (n.as_bytes(), "100644", blob.as_str()))
            .collect();
        let tree = tree(repo, &entries);
        let manifest = compute(repo, &empty, &tree).unwrap();
        let trailer = manifest.trailer_value().unwrap();
        if count == 20 {
            assert_eq!(trailer.split(',').count(), 20);
        } else {
            assert_eq!(trailer, "count=21;note=refs/notes/mai-objects@self");
        }
    }
}

#[test]
fn missing_objects_and_invalid_evidence_fail_closed() {
    let temp = repo();
    let empty = tree(temp.path(), &[]);
    assert!(compute(temp.path(), "missing", &empty).is_err());
    assert!(compute(temp.path(), "--all", &empty).is_err());
    let blob = blob(temp.path(), b"not tree");
    assert!(compute(temp.path(), &blob, &empty).is_err());
    let populated = tree(
        temp.path(),
        &[(b"b", "100644", &blob), (b"a", "100644", &blob)],
    );
    let manifest = compute(temp.path(), &empty, &populated).unwrap();
    let original = serde_json::to_value(manifest).unwrap();
    for version in [0, 2] {
        let mut value = original.clone();
        value["schema_version"] = version.into();
        assert!(serde_json::from_value::<FileManifest>(value).is_err());
    }
    for key in ["base_tree", "result_tree"] {
        let mut value = original.clone();
        value[key] = "not-an-oid".into();
        assert!(serde_json::from_value::<FileManifest>(value).is_err());
    }
    let mut zero = original.clone();
    zero["base_tree"] = "0".repeat(40).into();
    assert!(serde_json::from_value::<FileManifest>(zero).is_err());
    let mut sha256 = original.clone();
    sha256["base_tree"] = "a".repeat(64).into();
    sha256["result_tree"] = "b".repeat(64).into();
    assert!(serde_json::from_value::<FileManifest>(sha256).is_err());
    let mut duplicate = original.clone();
    duplicate["entries"][1] = duplicate["entries"][0].clone();
    assert!(serde_json::from_value::<FileManifest>(duplicate).is_err());
    let mut reversed = original.clone();
    reversed["entries"].as_array_mut().unwrap().reverse();
    assert!(serde_json::from_value::<FileManifest>(reversed).is_err());
    let mut parser = original.clone();
    parser["entries"][0]["parser"] = "invented-parser".into();
    assert!(serde_json::from_value::<FileManifest>(parser).is_err());
    let mut unknown = original;
    unknown["extra"] = true.into();
    assert!(serde_json::from_value::<FileManifest>(unknown).is_err());
}

#[test]
fn commits_resolve_to_exact_recursive_trees_without_checkout() {
    let temp = repo();
    let repo = temp.path();
    let blob = blob(repo, b"nested");
    let empty = tree(repo, &[]);
    let subtree = tree(repo, &[(b"file", "100644", &blob)]);
    let nested = tree(repo, &[(b"dir", "040000", &subtree)]);
    let base = git(repo, &["commit-tree", &empty, "-m", "base"], b"");
    let result = git(
        repo,
        &["commit-tree", &nested, "-p", &base, "-m", "result"],
        b"",
    );
    let manifest = compute(repo, &base, &result).unwrap();
    assert_eq!(manifest.base_tree, empty);
    assert_eq!(manifest.result_tree, nested);
    assert_eq!(manifest.entries.len(), 1);
    assert_eq!(manifest.entries[0].unit.canonical(), "file:dir/file");
    assert_eq!(manifest.entries[0].operation, Operation::Added);
    let reverse = compute(repo, &result, &base).unwrap();
    assert_eq!(reverse.entries[0].operation, Operation::Deleted);
    assert_eq!(git(repo, &["for-each-ref", "--format=%(refname)"], b""), "");
}
