use mai::lifecycle::{Lifecycle, Outcome, State};

#[test]
fn retry_registration_and_restart_preserve_exact_generation() {
    let mut lifecycle = Lifecycle::new();
    let commit = "a".repeat(40);
    let id = lifecycle
        .register("change", "alice", &commit, vec![])
        .unwrap();
    assert_eq!(
        id,
        lifecycle
            .register("change", "alice", &commit, vec![])
            .unwrap()
    );
    let mut restarted = Lifecycle::from_json(&lifecycle.to_json().unwrap()).unwrap();
    assert_eq!(
        id,
        restarted
            .register("change", "alice", &commit, vec![])
            .unwrap()
    );
    restarted.queue(&id).unwrap();
    let attempt = restarted.begin_attempt(&id).unwrap();
    restarted
        .complete_attempt(
            &id,
            attempt,
            Outcome::Landed {
                revision: 1,
                commit: "b".repeat(40),
            },
        )
        .unwrap();
    assert!(matches!(
        restarted.get(&id).unwrap().state(),
        State::Terminal { .. }
    ));
    assert_eq!(
        id,
        restarted
            .register("change", "alice", &commit, vec![])
            .unwrap()
    );
    assert_eq!(restarted.submissions().count(), 1);
}

use mai::lifecycle::{
    Annotation, AnnotationKind, AttemptStatus, Diagnostics, ExitCode, GenerationId,
    MAX_DIAGNOSTIC_BYTES, ReasonCode,
};
fn register(l: &mut Lifecycle, name: &str, c: char, deps: Vec<GenerationId>) -> GenerationId {
    l.register(name, "alice", &c.to_string().repeat(40), deps)
        .unwrap()
}
fn land(l: &mut Lifecycle, id: &GenerationId) {
    let n = l.begin_attempt(id).unwrap();
    l.complete_attempt(
        id,
        n,
        Outcome::Landed {
            revision: 1,
            commit: "f".repeat(40),
        },
    )
    .unwrap();
}
fn rejected(l: &Lifecycle, id: &GenerationId) -> bool {
    matches!(
        l.get(id).unwrap().state(),
        State::Terminal {
            outcome: Outcome::Rejected {
                reason: ReasonCode::DependencyFailed,
                ..
            }
        }
    )
}
#[test]
fn queued_update_supersedes_and_invalidates_exact_descendants() {
    let mut l = Lifecycle::new();
    let old = register(&mut l, "one", 'a', vec![]);
    l.queue(&old).unwrap();
    let child = register(&mut l, "two", 'b', vec![old.clone()]);
    l.queue(&child).unwrap();
    let new = register(&mut l, "one", 'c', vec![]);
    assert!(
        matches!(l.get(&old).unwrap().state(),State::Terminal {outcome:Outcome::Superseded {by}} if by==&new)
    );
    assert!(rejected(&l, &child));
    assert_eq!(l.get(&child).unwrap().dependencies(), &[old]);
    l.queue(&new).unwrap();
    land(&mut l, &new);
    l.validate().unwrap();
}
#[test]
fn validating_generation_finishes_before_latest_replacement() {
    let mut l = Lifecycle::new();
    let old = register(&mut l, "one", 'a', vec![]);
    l.queue(&old).unwrap();
    let attempt = l.begin_attempt(&old).unwrap();
    let replacement = register(&mut l, "one", 'b', vec![]);
    l.queue(&replacement).unwrap();
    assert_eq!(
        l.get(&replacement).unwrap().state(),
        &State::DependencyBlocked
    );
    assert!(l.begin_attempt(&replacement).is_err());
    let latest = register(&mut l, "one", 'c', vec![]);
    l.queue(&latest).unwrap();
    assert_eq!(l.get(&latest).unwrap().state(), &State::DependencyBlocked);
    assert!(l.begin_attempt(&latest).is_err());
    l.complete_attempt(
        &old,
        attempt,
        Outcome::Rejected {
            reason: ReasonCode::CheckFailed,
            diagnostics: Diagnostics::default(),
        },
    )
    .unwrap();
    assert_eq!(l.get(&latest).unwrap().state(), &State::Queued);
    land(&mut l, &latest);
    l.validate().unwrap();
}
#[test]
fn all_pending_parents_must_land_before_child() {
    let mut l = Lifecycle::new();
    let a = register(&mut l, "a", 'a', vec![]);
    let b = register(&mut l, "b", 'b', vec![]);
    let child = register(&mut l, "c", 'c', vec![b.clone(), a.clone()]);
    for id in [&a, &b, &child] {
        l.queue(id).unwrap();
    }
    assert!(l.begin_attempt(&child).is_err());
    land(&mut l, &a);
    assert!(l.begin_attempt(&child).is_err());
    land(&mut l, &b);
    land(&mut l, &child);
    l.validate().unwrap();
}
#[test]
fn cancellation_cascades_transitively_without_retargeting() {
    let mut l = Lifecycle::new();
    let a = register(&mut l, "a", 'a', vec![]);
    let b = register(&mut l, "b", 'b', vec![a.clone()]);
    let c = register(&mut l, "c", 'c', vec![b.clone()]);
    l.cancel(&a, Diagnostics::default()).unwrap();
    assert!(rejected(&l, &b));
    assert!(rejected(&l, &c));
    l.validate().unwrap();
}
#[test]
fn stale_attempt_cannot_finish_after_retry() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    l.queue(&id).unwrap();
    let n = l.begin_attempt(&id).unwrap();
    l.retry_attempt(&id, n).unwrap();
    assert!(!l.get(&id).unwrap().state().is_terminal());
    let newer = l.begin_attempt(&id).unwrap();
    assert_eq!(newer, n + 1);
    let outcome = Outcome::Landed {
        revision: 1,
        commit: "b".repeat(40),
    };
    assert!(l.complete_attempt(&id, n, outcome.clone()).is_err());
    assert!(l.retry_attempt(&id, n).is_err());
    l.complete_attempt(&id, newer, outcome).unwrap();
    assert_eq!(
        l.get(&id).unwrap().attempts()[0].status,
        AttemptStatus::Interrupted
    );
    l.validate().unwrap();
}
#[test]
fn awaiting_approval_is_separate_from_dependency_blocking() {
    let mut l = Lifecycle::new();
    let p = register(&mut l, "p", 'a', vec![]);
    let c = register(&mut l, "c", 'b', vec![p.clone()]);
    l.require_approval(&c).unwrap();
    assert!(l.begin_attempt(&c).is_err());
    l.approve(&c).unwrap();
    assert_eq!(l.get(&c).unwrap().state(), &State::DependencyBlocked);
    l.queue(&p).unwrap();
    land(&mut l, &p);
    assert_eq!(l.get(&c).unwrap().state(), &State::Queued);
    land(&mut l, &c);
    l.validate().unwrap();
}
#[test]
fn terminal_outcomes_are_immutable_but_annotations_append() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    l.queue(&id).unwrap();
    land(&mut l, &id);
    let state = l.get(&id).unwrap().state().clone();
    assert!(l.cancel(&id, Diagnostics::default()).is_err());
    assert!(
        l.reject(&id, ReasonCode::CheckFailed, Diagnostics::default())
            .is_err()
    );
    assert!(l.queue(&id).is_err());
    assert!(l.retry_attempt(&id, 1).is_err());
    l.annotate(
        &id,
        Annotation {
            kind: AnnotationKind::Revert,
            message: "Compensating change landed separately".into(),
        },
    )
    .unwrap();
    assert_eq!(l.get(&id).unwrap().state(), &state);
    assert_eq!(l.get(&id).unwrap().annotations().len(), 1);
    l.validate().unwrap();
}
#[test]
fn conflicting_retry_metadata_is_rejected_without_mutation() {
    let mut l = Lifecycle::new();
    let p = register(&mut l, "p", 'b', vec![]);
    register(&mut l, "a", 'a', vec![]);
    let before = l.to_json().unwrap();
    for (change, agent, deps) in [
        ("other", "alice", vec![]),
        ("a", "bob", vec![]),
        ("a", "alice", vec![p]),
    ] {
        assert!(l.register(change, agent, &"a".repeat(40), deps).is_err());
    }
    assert_eq!(l.to_json().unwrap(), before);
}
#[test]
fn same_change_cannot_change_agent() {
    let mut l = Lifecycle::new();
    register(&mut l, "a", 'a', vec![]);
    assert!(l.register("a", "bob", &"b".repeat(40), vec![]).is_err());
    assert_eq!(l.submissions().count(), 1);
}
#[test]
fn invalid_names_oids_and_dependencies_do_not_register() {
    let mut l = Lifecycle::new();
    for name in ["", "../x", "x.y", "hello world", "é", "x\n"] {
        assert!(l.register(name, "alice", &"a".repeat(40), vec![]).is_err());
    }
    for commit in [
        "a".repeat(39),
        "A".repeat(40),
        "0".repeat(40),
        "g".repeat(40),
    ] {
        assert!(l.register("a", "alice", &commit, vec![]).is_err());
    }
    assert!(
        l.register(
            "a",
            "alice",
            &"a".repeat(40),
            vec![GenerationId {
                change_id: "missing".into(),
                generation: 1
            }]
        )
        .is_err()
    );
    assert_eq!(l.submissions().count(), 0);
}
#[test]
fn duplicate_dependencies_reject() {
    let mut l = Lifecycle::new();
    let p = register(&mut l, "p", 'a', vec![]);
    assert!(
        l.register("c", "alice", &"b".repeat(40), vec![p.clone(), p])
            .is_err()
    );
}
#[test]
fn failed_parent_at_registration_immediately_rejects_child() {
    let mut l = Lifecycle::new();
    let p = register(&mut l, "p", 'a', vec![]);
    l.cancel(&p, Diagnostics::default()).unwrap();
    let child = register(&mut l, "c", 'b', vec![p]);
    assert!(rejected(&l, &child));
    l.validate().unwrap();
}
fn corrupt<F: FnOnce(&mut serde_json::Value)>(f: F) {
    let mut l = Lifecycle::new();
    register(&mut l, "a", 'a', vec![]);
    let mut json: serde_json::Value = serde_json::from_str(&l.to_json().unwrap()).unwrap();
    f(&mut json);
    assert!(Lifecycle::from_json(&json.to_string()).is_err());
}
#[test]
fn unknown_schema_rejects() {
    corrupt(|j| j["schema_version"] = 2.into());
}
#[test]
fn unknown_fields_reject() {
    corrupt(|j| j["submissions"][0]["priority"] = "sev".into());
}
#[test]
fn duplicate_generation_rejects() {
    corrupt(|j| {
        let s = j["submissions"][0].clone();
        j["submissions"].as_array_mut().unwrap().push(s);
    });
}
#[test]
fn missing_generation_rejects() {
    corrupt(|j| j["submissions"][0]["id"]["generation"] = 2.into());
}
#[test]
fn self_dependency_rejects() {
    corrupt(|j| {
        j["submissions"][0]["dependencies"] = serde_json::json!([{"change_id":"a","generation":1}])
    });
}
#[test]
fn orphan_running_attempt_rejects() {
    corrupt(|j| {
        j["submissions"][0]["attempts"] = serde_json::json!([{"number":1,"status":"running"}])
    });
}
#[test]
fn landing_without_attempt_rejects() {
    corrupt(
        |j| j["submissions"][0]["state"] = serde_json::json!({"kind":"terminal","outcome":{"kind":"landed","revision":1,"commit":"a".repeat(40)}}),
    );
}
#[test]
fn fabricated_approval_rejects() {
    corrupt(|j| j["submissions"][0]["state"] = serde_json::json!({"kind":"awaiting_approval"}));
}
#[test]
fn cyclic_snapshot_rejects() {
    let mut l = Lifecycle::new();
    let a = register(&mut l, "a", 'a', vec![]);
    register(&mut l, "b", 'b', vec![a]);
    let mut j: serde_json::Value = serde_json::from_str(&l.to_json().unwrap()).unwrap();
    j["submissions"][0]["dependencies"] = serde_json::json!([{"change_id":"b","generation":1}]);
    assert!(Lifecycle::from_json(&j.to_string()).is_err());
}
#[test]
fn bounded_diagnostics_preserve_unicode_and_reject_oversize() {
    let mut d = Diagnostics {
        log_tail: Some("é".repeat(MAX_DIAGNOSTIC_BYTES / 2)),
        ..Diagnostics::default()
    };
    assert!(d.validate().is_ok());
    d.log_tail.as_mut().unwrap().push('é');
    assert!(d.validate().is_err());
    d.log_tail = Some("nul\0".into());
    assert!(d.validate().is_err());
}
#[test]
fn candidate_refs_cannot_inject_or_escape() {
    for reference in [
        "refs/heads/main",
        "refs/mai/candidates/../head",
        "refs/mai/candidates/one\ntwo",
        "refs/mai/candidates/",
    ] {
        assert!(
            Diagnostics {
                candidate_ref: Some(reference.into()),
                ..Diagnostics::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Diagnostics {
            candidate_ref: Some("refs/mai/candidates/a/1/1".into()),
            ..Diagnostics::default()
        }
        .validate()
        .is_ok()
    );
}
#[test]
fn invalid_completion_does_not_consume_attempt() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    l.queue(&id).unwrap();
    let a = l.begin_attempt(&id).unwrap();
    assert!(
        l.complete_attempt(
            &id,
            a,
            Outcome::Landed {
                revision: 0,
                commit: "b".repeat(40)
            }
        )
        .is_err()
    );
    assert!(
        l.complete_attempt(
            &id,
            a,
            Outcome::Cancelled {
                diagnostics: Diagnostics::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        l.get(&id).unwrap().state(),
        &State::Validating { attempt: a }
    );
}
#[test]
fn cancellation_stops_active_attempt_and_stale_result_rejects() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    l.queue(&id).unwrap();
    let a = l.begin_attempt(&id).unwrap();
    l.cancel(&id, Diagnostics::default()).unwrap();
    assert_eq!(
        l.get(&id).unwrap().attempts()[0].status,
        AttemptStatus::Completed
    );
    assert!(
        l.complete_attempt(
            &id,
            a,
            Outcome::Landed {
                revision: 1,
                commit: "b".repeat(40)
            }
        )
        .is_err()
    );
    l.validate().unwrap();
}
#[test]
fn invalid_transitions_and_unknown_generation_reject() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    assert!(l.begin_attempt(&id).is_err());
    assert!(l.approve(&id).is_err());
    assert!(
        l.annotate(
            &id,
            Annotation {
                kind: AnnotationKind::Recovery,
                message: "no".into()
            }
        )
        .is_err()
    );
    l.queue(&id).unwrap();
    assert!(l.queue(&id).is_err());
    let n = l.begin_attempt(&id).unwrap();
    assert!(l.require_approval(&id).is_err());
    assert!(
        l.reject(&id, ReasonCode::CheckFailed, Diagnostics::default())
            .is_err()
    );
    assert!(l.retry_attempt(&id, n + 1).is_err());
    assert!(
        l.queue(&GenerationId {
            change_id: "missing".into(),
            generation: 1
        })
        .is_err()
    );
}
#[test]
fn reason_and_exit_codes_are_stable() {
    assert_eq!(
        serde_json::to_string(&ReasonCode::DependencyFailed).unwrap(),
        "\"dependency_failed\""
    );
    assert_eq!(State::Received.exit_code() as i32, 6);
    assert_eq!(
        Outcome::Landed {
            revision: 1,
            commit: "a".repeat(40)
        }
        .exit_code() as i32,
        0
    );
    assert_eq!(
        Outcome::Rejected {
            reason: ReasonCode::Infrastructure,
            diagnostics: Diagnostics::default()
        }
        .exit_code(),
        ExitCode::Infrastructure
    );
    assert_eq!(
        Outcome::Cancelled {
            diagnostics: Diagnostics::default()
        }
        .exit_code() as i32,
        3
    );
    assert_eq!(ExitCode::InvalidInput as i32, 2);
    assert_eq!(ExitCode::Unavailable as i32, 4);
}
#[test]
fn active_attempt_survives_snapshot_for_explicit_reconciliation() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    l.queue(&id).unwrap();
    let n = l.begin_attempt(&id).unwrap();
    let mut restored = Lifecycle::from_json(&l.to_json().unwrap()).unwrap();
    assert_eq!(
        restored.get(&id).unwrap().state(),
        &State::Validating { attempt: n }
    );
    restored.retry_attempt(&id, n).unwrap();
    assert_eq!(restored.begin_attempt(&id).unwrap(), n + 1);
}
#[test]
fn cli_version_and_retired_commands() {
    let binary = env!("CARGO_BIN_EXE_mai");
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "mai 0.0.1\n");
    let output = std::process::Command::new(binary)
        .arg("refresh")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
#[test]
fn restored_replacement_cannot_bypass_live_predecessor() {
    let mut l = Lifecycle::new();
    let old = register(&mut l, "a", 'a', vec![]);
    l.queue(&old).unwrap();
    l.begin_attempt(&old).unwrap();
    let new = register(&mut l, "a", 'b', vec![]);
    l.queue(&new).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(&l.to_json().unwrap()).unwrap();
    j["submissions"][1]["predecessor"] = serde_json::Value::Null;
    j["submissions"][1]["state"] = serde_json::json!({"kind":"queued"});
    assert!(Lifecycle::from_json(&j.to_string()).is_err());
}
#[test]
fn interrupted_predecessor_still_blocks_latest_replacement() {
    let mut l = Lifecycle::new();
    let old = register(&mut l, "a", 'a', vec![]);
    l.queue(&old).unwrap();
    let n = l.begin_attempt(&old).unwrap();
    let second = register(&mut l, "a", 'b', vec![]);
    l.queue(&second).unwrap();
    l.retry_attempt(&old, n).unwrap();
    let third = register(&mut l, "a", 'c', vec![]);
    l.queue(&third).unwrap();
    assert!(l.begin_attempt(&third).is_err());
    assert_eq!(l.get(&third).unwrap().predecessor(), Some(&old));
    let n = l.begin_attempt(&old).unwrap();
    l.complete_attempt(
        &old,
        n,
        Outcome::Landed {
            revision: 1,
            commit: "d".repeat(40),
        },
    )
    .unwrap();
    assert_eq!(l.get(&third).unwrap().state(), &State::Queued);
    l.validate().unwrap();
}
#[test]
fn oversize_rejection_does_not_mutate_state() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    let before = l.to_json().unwrap();
    assert!(
        l.reject(
            &id,
            ReasonCode::Conflict,
            Diagnostics {
                conflict: Some("x".repeat(MAX_DIAGNOSTIC_BYTES + 1)),
                ..Diagnostics::default()
            }
        )
        .is_err()
    );
    assert_eq!(l.to_json().unwrap(), before);
}
#[test]
fn empty_or_oversized_annotations_reject() {
    let mut l = Lifecycle::new();
    let id = register(&mut l, "a", 'a', vec![]);
    l.cancel(&id, Diagnostics::default()).unwrap();
    for message in [String::new(), "x".repeat(MAX_DIAGNOSTIC_BYTES + 1)] {
        assert!(
            l.annotate(
                &id,
                Annotation {
                    kind: AnnotationKind::Validation,
                    message
                }
            )
            .is_err()
        );
    }
    assert!(l.get(&id).unwrap().annotations().is_empty());
}
