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
    let receive_diff = diff(l, &base, &receive);
    let final_diff = diff(l, &base, &final_id);
    assert_eq!(receive_diff.len(), 1);
    assert_eq!(final_diff.len(), 2);
    for (view, id, deltas, snapshot) in [
        ("receive", receive, receive_diff, &submitted),
        (
            "final-post-regeneration",
            final_id,
            final_diff,
            &final_state,
        ),
    ] {
        for (kind, path) in deltas {
            let units = extract(&path, &snapshot[&path].1);
            writeln!(l.log,"MANIFEST view={view} base={base} commit={id} kind={kind} path={path} extractor=synthetic-grammar-v1 units={units:?}").unwrap();
        }
    }
    let mut renamed = b.clone();
    let value = renamed.remove("source.units").unwrap();
    renamed.insert("renamed.units".into(), value);
    let renamed_id = l.commit(&renamed, &[&base]);
    let d = diff(l, &base, &renamed_id);
    assert!(d.contains(&("D".into(), "source.units".into())));
    assert!(d.contains(&("A".into(), "renamed.units".into())));
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
