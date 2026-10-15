//! Incident overlay: independent recovery service, no queue-capacity feedback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attribution {
    Confirmed,
    Flaky,
    Uncertain,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Fixed,
    Reverted,
    FreezeNotify,
    AwaitEvidence,
}
#[derive(Clone, Debug)]
pub struct Case {
    pub attribution: Attribution,
    pub fix_delay: u64,
    pub gate_failures: usize,
    pub confirm_fails: bool,
    pub confirm_duration: u64,
    pub revert_conflict: bool,
    pub revert_gate_fails: bool,
}
#[derive(Debug)]
pub struct Recovery {
    pub outcome: Outcome,
    pub green: bool,
    pub finished: u64,
    pub deadline: u64,
    pub fix_attempts: usize,
    pub confirmations: usize,
    pub revert_attempts: usize,
    pub service: u64,
}
pub fn cases() -> Vec<Case> {
    let base = Case {
        attribution: Attribution::Confirmed,
        fix_delay: 120,
        gate_failures: 0,
        confirm_fails: false,
        confirm_duration: 120,
        revert_conflict: false,
        revert_gate_fails: false,
    };
    vec![
        base.clone(),
        Case {
            fix_delay: 600,
            ..base.clone()
        },
        Case {
            fix_delay: 1200,
            ..base.clone()
        },
        Case {
            gate_failures: 3,
            ..base.clone()
        },
        Case {
            confirm_fails: true,
            ..base.clone()
        },
        Case {
            confirm_duration: 1900,
            ..base.clone()
        },
        Case {
            fix_delay: 2000,
            revert_conflict: true,
            ..base.clone()
        },
        Case {
            fix_delay: 2000,
            revert_gate_fails: true,
            ..base.clone()
        },
        Case {
            attribution: Attribution::Flaky,
            ..base.clone()
        },
        Case {
            attribution: Attribution::Uncertain,
            ..base
        },
    ]
}
pub fn recover(case: &Case, begin: u64, window: u64) -> Recovery {
    // 120 seconds of synthetic before/after confirmation precedes any grant.
    let confirmed = begin + 120;
    let deadline = confirmed + window;
    let mut r = Recovery {
        outcome: Outcome::AwaitEvidence,
        green: false,
        finished: confirmed,
        deadline,
        fix_attempts: 0,
        confirmations: 0,
        revert_attempts: 0,
        service: 120,
    };
    if case.attribution != Attribution::Confirmed {
        return r;
    }
    let mut now = confirmed;
    if window > 0 {
        now = (confirmed + case.fix_delay).min(deadline);
        while now < deadline && r.fix_attempts < 3 {
            r.fix_attempts += 1;
            let gate_end = (now + 60).min(deadline);
            r.service += gate_end - now;
            now = gate_end;
            if now >= deadline {
                break;
            }
            if r.fix_attempts <= case.gate_failures {
                continue;
            }
            r.confirmations += 1;
            let end = (now + case.confirm_duration).min(deadline);
            r.service += end - now;
            let completed = now + case.confirm_duration <= deadline;
            now = end;
            if completed && !case.confirm_fails {
                r.outcome = Outcome::Fixed;
                r.green = true;
                r.finished = now;
                return r;
            }
            // Failure or timeout invokes backstop without resetting the deadline.
            break;
        }
    }
    if case.revert_conflict {
        r.outcome = Outcome::FreezeNotify;
        r.finished = now;
        return r;
    }
    r.revert_attempts = 1;
    now += 60;
    r.service += 60;
    if case.revert_gate_fails {
        r.outcome = Outcome::FreezeNotify;
    } else {
        r.outcome = Outcome::Reverted;
        r.green = true;
    }
    r.finished = now;
    r
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inconclusive_never_compensates_or_grants() {
        for case in cases()
            .into_iter()
            .filter(|c| c.attribution != Attribution::Confirmed)
        {
            let r = recover(&case, 0, 0);
            assert_eq!(r.outcome, Outcome::AwaitEvidence);
            assert_eq!(r.revert_attempts, 0);
            assert_eq!(r.fix_attempts, 0);
            assert!(!r.green);
        }
    }
    #[test]
    fn gate_pass_requires_full_confirmation_and_fixed_deadline() {
        let mut case = cases()[0].clone();
        let good = recover(&case, 100, 900);
        assert_eq!(good.outcome, Outcome::Fixed);
        assert_eq!(good.confirmations, 1);
        case.confirm_fails = true;
        let bad = recover(&case, 100, 900);
        assert_eq!(bad.outcome, Outcome::Reverted);
        assert_eq!(bad.deadline, good.deadline);
        case.confirm_fails = false;
        case.confirm_duration = 1900;
        let timeout = recover(&case, 100, 900);
        assert_eq!(timeout.finished, timeout.deadline + 60);
        assert_eq!(timeout.revert_attempts, 1);
    }
    #[test]
    fn failed_revert_freezes_and_attempt_budget_is_bounded() {
        let cs = cases();
        assert_eq!(recover(&cs[6], 0, 300).outcome, Outcome::FreezeNotify);
        assert_eq!(recover(&cs[7], 0, 300).outcome, Outcome::FreezeNotify);
        let r = recover(&cs[3], 0, 1800);
        assert_eq!(r.fix_attempts, 3);
        assert_eq!(r.outcome, Outcome::Reverted);
        assert!(r.finished < r.deadline);
    }
    #[test]
    fn immediate_revert_does_not_attempt_a_fix() {
        let r = recover(&cases()[0], 0, 0);
        assert_eq!(r.fix_attempts, 0);
        assert_eq!(r.finished, 180);
        assert_eq!(r.outcome, Outcome::Reverted);
    }
}
