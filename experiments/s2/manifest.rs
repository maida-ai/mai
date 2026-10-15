//! Restricted synthetic grammar, deliberately not a source-language parser.
use super::*;
#[derive(Debug, PartialEq, Eq)]
struct Unit {
    id: String,
    fallback: Option<&'static str>,
}
fn extract(path: &str, bytes: &[u8]) -> Vec<Unit> {
    let fallback = |reason| {
        vec![Unit {
            id: format!("file:{path}"),
            fallback: Some(reason),
        }]
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return fallback("binary");
    };
    if path.ends_with(".generated") {
        return fallback("generated");
    }
    if !path.ends_with(".units") {
        return fallback("unsupported grammar");
    }
    if text.trim().is_empty() {
        return fallback("empty grammar");
    }
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((kind, name)) = line.split_once(' ') else {
            return fallback("parse failure");
        };
        if !["symbol", "config", "module"].contains(&kind) || name.is_empty() || name.contains(' ')
        {
            return fallback("parse failure");
        }
        let id = format!("{kind}:{path}:{name}");
        if out.iter().any(|u: &Unit| u.id == id) {
            return fallback("ambiguous identity");
        }
        out.push(Unit { id, fallback: None });
    }
    out
}
fn contains(parent: &str, child: &str) -> bool {
    if let Some(path) = parent.strip_prefix("file:") {
        return child.split(':').nth(1) == Some(path);
    }
    child
        .strip_prefix(parent)
        .is_some_and(|tail| tail.starts_with('.'))
        || parent == child
}
fn diff(l: &mut Lab, from: &str, to: &str) -> Vec<(String, String)> {
    let raw = l.git(
        &[
            "diff-tree",
            "--no-commit-id",
            "--no-renames",
            "-r",
            "--name-status",
            from,
            to,
        ],
        b"",
    );
    raw.lines()
        .map(|line| {
            let (status, path) = line.split_once('\t').unwrap();
            (status.into(), path.into())
        })
        .collect()
}
#[derive(Debug, PartialEq, Eq)]
struct Change {
    kind: String,
    unit: Unit,
}
fn change(kind: &str, id: &str, fallback: Option<&'static str>) -> Change {
    Change {
        kind: kind.into(),
        unit: Unit {
            id: id.into(),
            fallback,
        },
    }
}
// File deltas come from Git, while units are compared on both sides. The grammar
// models identities only, not bodies; uncertain content/mode changes stay files.
fn manifest(before: &Snapshot, after: &Snapshot, paths: &[(String, String)]) -> Vec<Change> {
    let mut out = Vec::new();
    for (kind, path) in paths {
        let old = before.get(path);
        let new = after.get(path);
        let left = old.map(|v| extract(path, &v.1)).unwrap_or_default();
        let right = new.map(|v| extract(path, &v.1)).unwrap_or_default();
        let reason = left
            .iter()
            .chain(&right)
            .find_map(|u| u.fallback)
            .or_else(|| match (old, new) {
                (Some(a), Some(b)) if a.0 != b.0 => Some("mode change"),
                (Some(a), Some(b)) if a != b && left == right => Some("identity-only grammar"),
                _ => None,
            });
        if let Some(reason) = reason {
            out.push(change(kind, &format!("file:{path}"), Some(reason)));
            continue;
        }
        for unit in &left {
            if !right.contains(unit) {
                out.push(change("D", &unit.id, None));
            }
        }
        for unit in &right {
            if !left.contains(unit) {
                out.push(change("A", &unit.id, None));
            }
        }
    }
    out
}
fn assert_manifest(
    l: &mut Lab,
    view: &str,
    base: &str,
    id: &str,
    before: &Snapshot,
    after: &Snapshot,
    expected: Vec<Change>,
) {
    let paths = diff(l, base, id);
    let actual = manifest(before, after, &paths);
    writeln!(l.log, "MANIFEST view={view} base={base} commit={id} extractor=synthetic-grammar-v1 changes={actual:?}").unwrap();
    assert_eq!(actual, expected, "complete {view} manifest");
}
pub(super) fn manifests(l: &mut Lab) {
    let mut b = Snapshot::new();
    put(
        &mut b,
        "source.units",
        b"module pkg\nsymbol Class\nsymbol Class.method\nconfig database.host\n",
    );
    let base = l.commit(&b, &[]);
    let mut submitted = b.clone();
    put(
        &mut submitted,
        "source.units",
        b"module pkg\nsymbol Class\nsymbol Class.method\nconfig database.port\n",
    );
    let receive = l.commit(&submitted, &[&base]);
    let mut final_state = submitted.clone();
    put(&mut final_state, "cache.generated", b"generated output");
    let final_id = l.commit(&final_state, &[&base]);
    let expected_receive = || {
        vec![
            change("D", "config:source.units:database.host", None),
            change("A", "config:source.units:database.port", None),
        ]
    };
    assert_manifest(
        l,
        "receive",
        &base,
        &receive,
        &b,
        &submitted,
        expected_receive(),
    );
    let mut expected_final = vec![change("A", "file:cache.generated", Some("generated"))];
    expected_final.extend(expected_receive());
    assert_manifest(
        l,
        "final-post-regeneration",
        &base,
        &final_id,
        &b,
        &final_state,
        expected_final,
    );
    let mut renamed = b.clone();
    let value = renamed.remove("source.units").unwrap();
    renamed.insert("renamed.units".into(), value);
    let renamed_id = l.commit(&renamed, &[&base]);
    let deleted = vec![
        change("D", "module:source.units:pkg", None),
        change("D", "symbol:source.units:Class", None),
        change("D", "symbol:source.units:Class.method", None),
        change("D", "config:source.units:database.host", None),
    ];
    let mut expected_rename = vec![
        change("A", "module:renamed.units:pkg", None),
        change("A", "symbol:renamed.units:Class", None),
        change("A", "symbol:renamed.units:Class.method", None),
        change("A", "config:renamed.units:database.host", None),
    ];
    expected_rename.extend(
        deleted
            .iter()
            .map(|c| change(&c.kind, &c.unit.id, c.unit.fallback)),
    );
    assert_manifest(
        l,
        "rename",
        &base,
        &renamed_id,
        &b,
        &renamed,
        expected_rename,
    );
    let empty = Snapshot::new();
    let deleted_id = l.commit(&empty, &[&base]);
    assert_manifest(l, "deletion", &base, &deleted_id, &b, &empty, deleted);
    assert!(contains(
        "file:source.units",
        "symbol:source.units:Class.method"
    ));
    assert!(contains(
        "symbol:source.units:Class",
        "symbol:source.units:Class.method"
    ));
    assert!(!contains(
        "symbol:source.units:Class",
        "symbol:source.units:Classes"
    ));
    for (path, text, reason) in [
        ("x.generated", b"x".as_slice(), "generated"),
        ("x.rs", b"fn x() {}", "unsupported grammar"),
        ("x.units", b"symbol x\nsymbol x\n", "ambiguous identity"),
        ("x.units", b"malformed", "parse failure"),
    ] {
        assert_eq!(extract(path, text)[0].fallback, Some(reason));
    }
    writeln!(l.log,"PASS restricted units, containment, fallback, rename delete-plus-add, receive/final diff recomputation. Approximation never authorizes semantic independence; real parser qualification PENDING.").unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;
    fn changes(before: &[u8], after: &[u8]) -> Vec<Change> {
        let mut b = Snapshot::new();
        let mut a = Snapshot::new();
        put(&mut b, "x.units", before);
        put(&mut a, "x.units", after);
        manifest(&b, &a, &[("M".into(), "x.units".into())])
    }
    #[test]
    fn replacement_preserves_removed_unit_and_excludes_unchanged() {
        assert_eq!(
            changes(b"symbol same\nconfig host", b"symbol same\nconfig port"),
            vec![
                change("D", "config:x.units:host", None),
                change("A", "config:x.units:port", None),
            ]
        );
    }
    #[test]
    fn uncertain_before_or_after_requires_file_fallback() {
        for (before, after) in [
            (b"bad".as_slice(), b"symbol good".as_slice()),
            (b"symbol good".as_slice(), b"bad".as_slice()),
        ] {
            assert_eq!(
                changes(before, after),
                vec![change("M", "file:x.units", Some("parse failure"))]
            );
        }
    }
    #[test]
    fn mode_only_change_is_not_an_empty_manifest() {
        let mut b = Snapshot::new();
        put(&mut b, "x.units", b"symbol same");
        let mut a = b.clone();
        a.get_mut("x.units").unwrap().0 = "100755".into();
        assert_eq!(
            manifest(&b, &a, &[("M".into(), "x.units".into())]),
            vec![change("M", "file:x.units", Some("mode change"))]
        );
    }
    #[test]
    fn generated_widening_and_deletion_keep_fallback() {
        let empty = Snapshot::new();
        let mut generated = Snapshot::new();
        put(&mut generated, "cache.generated", b"output");
        for (before, after, kind) in [(&empty, &generated, "A"), (&generated, &empty, "D")] {
            assert_eq!(
                manifest(before, after, &[(kind.into(), "cache.generated".into())]),
                vec![change(kind, "file:cache.generated", Some("generated"))]
            );
        }
    }
    #[test]
    fn unmodeled_content_change_keeps_file_fallback() {
        assert_eq!(
            changes(b"symbol same", b"symbol same\n"),
            vec![change("M", "file:x.units", Some("identity-only grammar"))]
        );
    }
    #[test]
    fn containment_boundaries() {
        assert!(contains("symbol:x:C", "symbol:x:C.f"));
        assert!(!contains("symbol:x:C", "symbol:y:C.f"));
        assert!(!contains("symbol:x:C", "symbol:x:Cat"));
    }
    #[test]
    fn conservative_errors() {
        for bytes in [
            b"".as_slice(),
            b"bad".as_slice(),
            b"symbol a\nsymbol a",
            b"\xff",
        ] {
            let units = extract("x.units", bytes);
            assert_eq!(units.len(), 1);
            assert!(units[0].fallback.is_some());
        }
    }
    #[test]
    fn units() {
        assert_eq!(extract("x.units", b"symbol a\nconfig b\nmodule c").len(), 3);
    }
}
