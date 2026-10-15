//! Synthetic S4 research model. No product dependencies, Git, or real validation.
use std::{
    env, fs,
    io::{self, Write},
    path::Path,
    time::Instant,
};
mod input;
mod policy;
mod recovery;
const AGENTS: usize = 30;
const CONFIG: u64 = 0x5344;
const TICK: u64 = 10;
const AGING: u64 = 600;
const GRACE: u64 = 30;
const SEV_CAP: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Class {
    Low,
    Normal,
    High,
    Sev,
}
impl Class {
    fn name(self) -> &'static str {
        ["low", "normal", "high", "sev"][self as usize]
    }
    fn age(self, steps: u64) -> Self {
        if self == Self::Sev {
            return self;
        }
        [Self::Low, Self::Normal, Self::High][(self as usize + steps as usize).min(2)]
    }
}
#[derive(Clone, Debug)]
struct Grant {
    subject: usize,
    class: Class,
    issued: u64,
    expires: u64,
    revoked: Option<u64>,
}
impl Grant {
    fn valid(&self, subject: usize, now: u64) -> bool {
        self.subject == subject
            && now >= self.issued
            && now < self.expires
            && self.revoked.is_none_or(|t| now < t)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Waiting,
    Running,
    Landed(u64),
    Rejected,
}
#[derive(Clone, Debug)]
struct Job {
    id: usize,
    agent: usize,
    arrival: u64,
    requested: Class,
    grant: Option<Grant>,
    dep: Option<usize>,
    duration: u64,
    fails: bool,
    flakes: bool,
    conflict: bool,
    state: State,
    aged: bool,
    inherited: bool,
}
impl Job {
    fn own_class(&self, now: u64) -> Class {
        let base = if self.requested >= Class::High {
            self.grant
                .as_ref()
                .filter(|g| g.valid(self.id, now))
                .map_or(Class::Normal, |g| g.class)
        } else {
            self.requested
        };
        base.age(now.saturating_sub(self.arrival) / AGING)
    }
}
#[derive(Clone, Copy, Debug)]
enum Mode {
    Sequential,
    Pipeline(usize),
    Batch(usize),
}
impl Mode {
    fn name(self) -> String {
        match self {
            Self::Sequential => "sequential".into(),
            Self::Pipeline(n) => format!("pipeline-{n}"),
            Self::Batch(n) => format!("batch-{n}"),
        }
    }
    fn slots(self) -> usize {
        match self {
            Self::Pipeline(n) => n,
            _ => 1,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Candidate {
    tree: u64,
    parent: u64,
    config: u64,
}
#[derive(Clone, Debug)]
struct Attempt {
    members: Vec<usize>,
    candidate: Candidate,
    start: u64,
    end: u64,
    class: Class,
    retry: bool,
    passed: bool,
    evidence: Option<Candidate>,
}
#[derive(Default, Debug)]
struct Metrics {
    busy: u64,
    waste: u64,
    preemptions: u64,
    retries: u64,
    invalidated: u64,
    batch_splits: u64,
    cap_rebuilds: u64,
    peak_sev: usize,
    lease_waits: u64,
    aged: u64,
    inherited: u64,
    grants: u64,
    expired: u64,
    revoked: u64,
    pending_at_horizon: usize,
}
struct ResultRun {
    jobs: Vec<Job>,
    metrics: Metrics,
    end: u64,
    landings: Vec<u64>,
}
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pct(&mut self, n: u64) -> bool {
        self.next() % 100 < n
    }
}
fn workload(seed: u64, interval: u64, failure: u64, severe_mix: bool) -> Vec<Job> {
    let mut rng = Rng(seed);
    let mut jobs = Vec::new();
    // Ten waves retain repeated arrivals from all 30 agents. Every fifth wave
    // chains to that agent's prior submission, including low-to-severe inversions.
    for wave in 0..10 {
        for agent in 0..AGENTS {
            let id = jobs.len();
            let arrival = wave * interval
                + agent as u64 * interval / AGENTS as u64
                + rng.next() % (interval / 10 + 1);
            let p = rng.next() % 100;
            let requested = if p < if severe_mix { 25 } else { 5 } {
                Class::Sev
            } else if p < 35 {
                Class::High
            } else if p < 80 {
                Class::Normal
            } else {
                Class::Low
            };
            let grant = (requested >= Class::High).then(|| Grant {
                subject: id,
                class: requested,
                issued: arrival,
                expires: arrival + 900,
                revoked: rng.pct(10).then_some(arrival + 120),
            });
            jobs.push(Job {
                id,
                agent,
                arrival,
                requested,
                grant,
                dep: (wave > 0 && wave % 3 == 0).then(|| id - AGENTS),
                duration: (3 + rng.next() % 7) * TICK,
                fails: rng.pct(failure),
                flakes: rng.pct(failure / 2),
                conflict: rng.pct(failure),
                state: State::Waiting,
                aged: false,
                inherited: false,
            });
        }
    }
    jobs
}
fn effective(jobs: &[Job], now: u64) -> Vec<Class> {
    let mut classes: Vec<_> = jobs.iter().map(|j| j.own_class(now)).collect();
    // Dependencies always point to earlier IDs; reverse traversal propagates chains.
    for j in jobs
        .iter()
        .rev()
        .filter(|j| j.arrival <= now && matches!(j.state, State::Waiting | State::Running))
    {
        if let Some(parent) = j.dep {
            if matches!(jobs[parent].state, State::Waiting | State::Running) {
                classes[parent] = classes[parent].max(classes[j.id]);
            }
        }
    }
    classes
}
fn ready(j: &Job, jobs: &[Job], now: u64) -> bool {
    j.state == State::Waiting
        && j.arrival <= now
        && (!j.conflict || now >= j.arrival + GRACE)
        && j.dep
            .is_none_or(|p| matches!(jobs[p].state, State::Landed(_)))
}
fn valid_evidence(candidate: Candidate, evidence: Candidate, passed: bool, trunk: u64) -> bool {
    passed && candidate == evidence && candidate.parent == trunk && candidate.config == CONFIG
}
fn cancel_from(
    active: &mut Vec<Attempt>,
    at: usize,
    jobs: &mut [Job],
    metrics: &mut Metrics,
    now: u64,
) {
    for a in active.drain(at..) {
        metrics.waste += now.min(a.end).saturating_sub(a.start);
        metrics.invalidated += 1;
        for id in a.members {
            jobs[id].state = State::Waiting;
        }
    }
}
fn simulate(mut jobs: Vec<Job>, mode: Mode, trace: &mut impl Write) -> io::Result<ResultRun> {
    let mut metrics = Metrics {
        grants: jobs.iter().filter(|j| j.grant.is_some()).count() as u64,
        ..Metrics::default()
    };
    for j in &jobs {
        writeln!(
            trace,
            "input id={} agent={} arrival={} class={} dep={:?} gate={} fail={} flake={} conflict={}",
            j.id,
            j.agent,
            j.arrival,
            j.requested.name(),
            j.dep,
            j.duration,
            j.fails,
            j.flakes,
            j.conflict
        )?;
        if let Some(g) = &j.grant {
            writeln!(
                trace,
                "grant subject={} issuer=synthetic-human reason=fixture scope=change class={} issued={} expiry={} revoke={:?}",
                g.subject,
                g.class.name(),
                g.issued,
                g.expires,
                g.revoked
            )?;
        }
    }
    let horizon = jobs.iter().map(|j| j.arrival).max().unwrap_or(0);
    let limit = horizon + 120_000;
    let mut now = 0;
    let mut trunk = 0;
    let mut serial = 0;
    let mut active: Vec<Attempt> = Vec::new();
    let mut landings = Vec::new();
    let mut split_batch = false;
    while now <= limit {
        if now >= horizon && metrics.pending_at_horizon == 0 {
            metrics.pending_at_horizon = jobs
                .iter()
                .filter(|j| matches!(j.state, State::Waiting | State::Running))
                .count();
        }
        for id in 0..jobs.len() {
            if jobs[id].state == State::Waiting
                && jobs[id]
                    .dep
                    .is_some_and(|p| jobs[p].state == State::Rejected)
            {
                jobs[id].state = State::Rejected;
                writeln!(trace, "t={now} reject-dependent id={id}")?;
            }
        }
        // Failures invalidate all speculative descendants. Flakes retry the exact
        // immutable candidate; there is no regenerated identity on retry.
        let mut failure = None;
        for (i, a) in active.iter_mut().enumerate() {
            if a.end <= now && !a.passed {
                if a.members.iter().any(|id| jobs[*id].flakes) && !a.retry {
                    a.retry = true;
                    metrics.retries += 1;
                    metrics.waste += a.end - a.start;
                    a.start = now;
                    a.end = now + a.members.iter().map(|id| jobs[*id].duration).max().unwrap();
                    writeln!(
                        trace,
                        "t={now} retry-same-candidate {:?} members={:?}",
                        a.candidate, a.members
                    )?;
                } else if a.members.iter().any(|id| jobs[*id].fails) {
                    failure = Some(i);
                    break;
                } else {
                    a.passed = true;
                    a.evidence = Some(a.candidate);
                    writeln!(
                        trace,
                        "t={now} pass {:?} retry={} members={:?}",
                        a.candidate, a.retry, a.members
                    )?;
                }
            }
        }
        if let Some(i) = failure {
            let bad = active[i].members.clone();
            writeln!(
                trace,
                "t={now} failed {:?} members={bad:?}",
                active[i].candidate
            )?;
            cancel_from(&mut active, i, &mut jobs, &mut metrics, now);
            if bad.len() > 1 {
                split_batch = true;
                metrics.batch_splits += 1;
            } else {
                jobs[bad[0]].state = State::Rejected;
            }
        }
        while active.first().is_some_and(|a| a.passed) {
            let a = active.remove(0);
            assert!(valid_evidence(
                a.candidate,
                a.evidence.expect("missing pass evidence"),
                a.passed,
                trunk
            ));
            trunk = a.candidate.tree;
            for id in &a.members {
                jobs[*id].state = State::Landed(now);
                landings.push(now);
            }
            writeln!(
                trace,
                "t={now} land {:?} scope={} members={:?}",
                a.candidate,
                if a.members.len() > 1 {
                    "batch-tip-only;intermediates-not-separately-validated"
                } else {
                    "exact-tree-parent-config"
                },
                a.members
            )?;
        }
        let classes = effective(&jobs, now);
        // Recompute running membership after arrivals, grant expiry/revocation and
        // dependency inheritance. A newly over-cap suffix is invalidated/requeued;
        // this is explicit model cap reconciliation, not a passing validation.
        let mut severe_members = 0;
        let mut over_cap = None;
        for (i, a) in active.iter_mut().enumerate() {
            a.class = a.members.iter().map(|id| classes[*id]).max().unwrap();
            severe_members += a
                .members
                .iter()
                .filter(|id| classes[**id] == Class::Sev)
                .count();
            if severe_members > SEV_CAP {
                over_cap = Some(i);
                break;
            }
        }
        if let Some(i) = over_cap {
            metrics.cap_rebuilds += 1;
            if active[i].members.len() > 1 {
                split_batch = true;
            }
            writeln!(
                trace,
                "t={now} cap-reconcile candidate={:?}",
                active[i].candidate
            )?;
            cancel_from(&mut active, i, &mut jobs, &mut metrics, now);
        }
        for j in &mut jobs {
            if j.arrival <= now && matches!(j.state, State::Waiting | State::Running) {
                if !j.aged
                    && j.own_class(now)
                        > if j.requested >= Class::High {
                            Class::Normal
                        } else {
                            j.requested
                        }
                    && now >= j.arrival + AGING
                {
                    j.aged = true;
                    metrics.aged += 1;
                    writeln!(
                        trace,
                        "t={now} aging id={} effective={}",
                        j.id,
                        classes[j.id].name()
                    )?;
                }
                if !j.inherited && classes[j.id] > j.own_class(now) {
                    j.inherited = true;
                    metrics.inherited += 1;
                    writeln!(
                        trace,
                        "t={now} inheritance id={} effective={}",
                        j.id,
                        classes[j.id].name()
                    )?;
                }
            }
        }
        let sev_ready = jobs
            .iter()
            .any(|j| ready(j, &jobs, now) && classes[j.id] == Class::Sev);
        // Only speculative attempts can be preempted, and their submissions survive.
        if sev_ready
            && active.len() == mode.slots()
            && active.iter().filter(|a| a.class == Class::Sev).count() < SEV_CAP
        {
            if let Some(i) = active
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(i, a)| (a.class < Class::Sev).then_some(i))
            {
                metrics.preemptions += 1;
                writeln!(
                    trace,
                    "t={now} preempt-attempt candidate={:?}",
                    active[i].candidate
                )?;
                cancel_from(&mut active, i, &mut jobs, &mut metrics, now);
            }
        }
        while active.len() < mode.slots() {
            let sev_running = active.iter().filter(|a| a.class == Class::Sev).count();
            let mut eligible: Vec<_> = jobs
                .iter()
                .filter(|j| {
                    ready(j, &jobs, now) && (classes[j.id] != Class::Sev || sev_running < SEV_CAP)
                })
                .map(|j| j.id)
                .collect();
            eligible.sort_by_key(|id| (std::cmp::Reverse(classes[*id]), jobs[*id].arrival, *id));
            if eligible.is_empty() {
                break;
            }
            let count = match mode {
                Mode::Batch(n) if !split_batch => n,
                _ => 1,
            };
            // Count severe members, including inherited prerequisites, against cap.
            let mut members = Vec::new();
            let mut severe = sev_running;
            for id in eligible {
                if members.len() == count {
                    break;
                }
                if classes[id] == Class::Sev {
                    if severe == SEV_CAP {
                        continue;
                    }
                    severe += 1;
                }
                members.push(id);
            }
            let class = members.iter().map(|id| classes[*id]).max().unwrap();
            let duration = members.iter().map(|id| jobs[*id].duration).max().unwrap();
            for id in &members {
                jobs[*id].state = State::Running;
                if jobs[*id].conflict {
                    metrics.lease_waits += 1;
                }
            }
            serial += 1;
            let candidate = Candidate {
                tree: serial,
                parent: active.last().map_or(trunk, |a| a.candidate.tree),
                config: CONFIG,
            };
            writeln!(
                trace,
                "t={now} start {candidate:?} class={} members={members:?}",
                class.name()
            )?;
            active.push(Attempt {
                members,
                candidate,
                start: now,
                end: now + duration,
                class,
                retry: false,
                passed: false,
                evidence: None,
            });
            metrics.peak_sev = metrics.peak_sev.max(severe);
        }
        if jobs
            .iter()
            .all(|j| matches!(j.state, State::Landed(_) | State::Rejected))
        {
            break;
        }
        metrics.busy += active.iter().filter(|a| !a.passed).count() as u64 * TICK;
        now += TICK;
    }
    if now > limit {
        return Err(io::Error::other(
            "drain exceeded bound; retain trace as failure evidence",
        ));
    }
    for j in &jobs {
        if let Some(g) = &j.grant {
            let terminal = match j.state {
                State::Landed(t) => t,
                _ => now,
            };
            metrics.expired += u64::from(terminal >= g.expires);
            metrics.revoked += u64::from(g.revoked.is_some_and(|t| terminal >= t));
        }
        writeln!(
            trace,
            "terminal id={} class={} state={:?}",
            j.id,
            j.requested.name(),
            j.state
        )?;
    }
    assert!(metrics.peak_sev <= SEV_CAP);
    Ok(ResultRun {
        jobs,
        metrics,
        end: now,
        landings,
    })
}
fn percentile(mut values: Vec<u64>, percent: usize) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[(values.len() * percent).div_ceil(100).saturating_sub(1)])
}
fn number(n: Option<u64>) -> String {
    n.map_or_else(|| "NA".into(), |n| n.to_string())
}
fn run(root: &Path, imported: Option<&Path>) -> io::Result<()> {
    fs::create_dir(root)?;
    let start = Instant::now();
    fs::write(root.join("policy.log"), policy::exercise())?;
    let mut metrics = fs::File::create(root.join("metrics.log"))?;
    let mut recoveries = fs::File::create(root.join("recovery.log"))?;
    writeln!(
        metrics,
        "scenario,seed,interval_s,failure_pct,severe_mix,mode,class,submitted,landed,rejected,p50_s,p95_s,max_s,end_s,utilization,waste_slot_s,preemptions,retries,invalidated,batch_splits,cap_rebuilds,peak_sev,aging,inheritance,grants,expired,revoked,grant_human_seconds,backlog_at_arrival_horizon,nominal_single_slot_load"
    )?;
    writeln!(
        recoveries,
        "scenario,mode,window_s,incident,attribution,outcome,time_to_green_s,stacked_on_red,fix_gate_attempts,full_confirmations,revert_gate_attempts,recovery_slot_seconds,deadline_s,finished_s"
    )?;
    let mut scenarios = 0;
    let mut inputs = Vec::new();
    if let Some(path) = imported {
        let contents = fs::read_to_string(path)?;
        let jobs = input::parse(&contents)?;
        fs::write(root.join("input.log"), contents)?;
        inputs.push((0, 0, 0, false, jobs));
    } else {
        let contents = fs::read_to_string("experiments/s4/grid.csv")?;
        fs::write(root.join("input.log"), &contents)?;
        if contents.lines().next() != Some("seed,interval_s,failure_pct,severe_mix") {
            return Err(io::Error::other("unsupported grid CSV header"));
        }
        for line in contents.lines().skip(1) {
            let c: Vec<_> = line.split(',').collect();
            if c.len() != 4 {
                return Err(io::Error::other("grid requires four columns"));
            }
            let seed: u64 = c[0].parse().map_err(io::Error::other)?;
            let interval: u64 = c[1].parse().map_err(io::Error::other)?;
            let failure: u64 = c[2].parse().map_err(io::Error::other)?;
            let mix: bool = c[3].parse().map_err(io::Error::other)?;
            if seed == 0 || !(60..=12000).contains(&interval) || failure > 100 {
                return Err(io::Error::other("invalid grid range"));
            }
            inputs.push((
                seed,
                interval,
                failure,
                mix,
                workload(seed, interval, failure, mix),
            ));
        }
    }
    for (seed, interval, failure, mix, input) in inputs {
        for mode in [
            Mode::Sequential,
            Mode::Pipeline(2),
            Mode::Pipeline(4),
            Mode::Batch(4),
        ] {
            scenarios += 1;
            let name = format!("s{scenarios:03}");
            let mut trace = io::BufWriter::new(fs::File::create(root.join(format!("{name}.log")))?);
            let result = simulate(input.clone(), mode, &mut trace)?;
            for class in [Class::Low, Class::Normal, Class::High, Class::Sev] {
                let js: Vec<_> = result
                    .jobs
                    .iter()
                    .filter(|j| j.requested == class)
                    .collect();
                let latencies: Vec<_> = js
                    .iter()
                    .filter_map(|j| {
                        if let State::Landed(t) = j.state {
                            Some(t - j.arrival)
                        } else {
                            None
                        }
                    })
                    .collect();
                let m = &result.metrics;
                writeln!(
                    metrics,
                    "{name},{seed},{interval},{failure},{mix},{},{},{},{},{},{},{},{},{},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.3}",
                    mode.name(),
                    class.name(),
                    js.len(),
                    latencies.len(),
                    js.iter().filter(|j| j.state == State::Rejected).count(),
                    number(percentile(latencies.clone(), 50)),
                    number(percentile(latencies.clone(), 95)),
                    number(latencies.iter().max().copied()),
                    result.end,
                    m.busy as f64 / (result.end.max(1) * mode.slots() as u64) as f64,
                    m.waste,
                    m.preemptions,
                    m.retries,
                    m.invalidated,
                    m.batch_splits,
                    m.cap_rebuilds,
                    m.peak_sev,
                    m.aged,
                    m.inherited,
                    m.grants,
                    m.expired,
                    m.revoked,
                    m.grants * 60,
                    m.pending_at_horizon,
                    if interval == 0 {
                        f64::NAN
                    } else {
                        AGENTS as f64 * 60.0 / interval as f64
                    }
                )?;
            }
            // Overlay incident service on the same observed landing timestamps.
            // Recovery service is NOT fed back into queue resource utilization.
            for window in [0, 300, 900, 1800] {
                for (incident, case) in recovery::cases().into_iter().enumerate() {
                    let begin = result
                        .landings
                        .get(result.landings.len() / 3)
                        .copied()
                        .unwrap_or(0);
                    let r = recovery::recover(&case, begin, window);
                    let stacked = result
                        .landings
                        .iter()
                        .filter(|t| {
                            **t > begin && **t <= if r.green { r.finished } else { result.end }
                        })
                        .count();
                    writeln!(
                        recoveries,
                        "{name},{},{window},{incident},{:?},{:?},{},{stacked},{},{},{},{},{},{}",
                        mode.name(),
                        case.attribution,
                        r.outcome,
                        if r.green {
                            (r.finished - begin).to_string()
                        } else {
                            "NA".into()
                        },
                        r.fix_attempts,
                        r.confirmations,
                        r.revert_attempts,
                        r.service,
                        r.deadline,
                        r.finished
                    )?;
                }
            }
            println!(
                "{name} {} seed={seed} interval={interval} failure={failure} severe_mix={mix} completed",
                mode.name()
            );
        }
    }
    writeln!(
        metrics,
        "# scenarios={scenarios}; elapsed_wall_ms={}; units=simulated-seconds; no real gates; proposed synthetic grid",
        start.elapsed().as_millis()
    )?;
    Ok(())
}
fn main() -> io::Result<()> {
    let args: Vec<_> = env::args().collect();
    if !(3..=4).contains(&args.len()) || args[1] != "run" {
        return Err(io::Error::other("usage: s4 run <new-root> [workload.csv]"));
    }
    run(Path::new(&args[2]), args.get(3).map(Path::new))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn job(id: usize, class: Class) -> Job {
        Job {
            id,
            agent: id % AGENTS,
            arrival: 0,
            requested: class,
            grant: (class >= Class::High).then_some(Grant {
                subject: id,
                class,
                issued: 0,
                expires: 10000,
                revoked: None,
            }),
            dep: None,
            duration: 60,
            fails: false,
            flakes: false,
            conflict: false,
            state: State::Waiting,
            aged: false,
            inherited: false,
        }
    }
    #[test]
    fn grant_expiry_revocation_identity_and_aging_cap() {
        let mut j = job(0, Class::Sev);
        assert_eq!(j.own_class(0), Class::Sev);
        j.grant.as_mut().unwrap().revoked = Some(100);
        assert_eq!(j.own_class(100), Class::Normal);
        assert_eq!(j.own_class(100000), Class::High);
        assert!(!j.grant.as_ref().unwrap().valid(1, 0));
        j.grant.as_mut().unwrap().revoked = None;
        assert!(!j.grant.as_ref().unwrap().valid(0, 10000));
    }
    #[test]
    fn dependency_chain_inherits_then_releases_without_cap_deadlock() {
        let mut jobs = vec![
            job(0, Class::Low),
            job(1, Class::Normal),
            job(2, Class::Sev),
        ];
        jobs[1].dep = Some(0);
        jobs[2].dep = Some(1);
        assert_eq!(effective(&jobs, 0), vec![Class::Sev; 3]);
        jobs[2].state = State::Rejected;
        assert_eq!(effective(&jobs, 0)[0], Class::Normal);
        jobs[2].state = State::Waiting;
        let r = simulate(jobs, Mode::Pipeline(2), &mut Vec::new()).unwrap();
        assert!(r.jobs.iter().all(|j| matches!(j.state, State::Landed(_))));
        assert_eq!(r.metrics.peak_sev, 1);
    }
    #[test]
    fn stale_parent_config_tree_and_failed_results_never_pass() {
        let c = Candidate {
            tree: 2,
            parent: 1,
            config: CONFIG,
        };
        assert!(valid_evidence(c, c, true, 1));
        assert!(!valid_evidence(c, c, true, 0));
        assert!(!valid_evidence(c, c, false, 1));
        assert!(!valid_evidence(c, Candidate { tree: 3, ..c }, true, 1));
        assert!(!valid_evidence(c, Candidate { config: 5, ..c }, true, 1));
    }
    #[test]
    fn flakes_retry_same_candidate_and_failure_invalidates_descendants() {
        let mut jobs = vec![job(0, Class::Normal), job(1, Class::Normal)];
        jobs[0].flakes = true;
        let mut trace = Vec::new();
        let r = simulate(jobs.clone(), Mode::Pipeline(2), &mut trace).unwrap();
        assert_eq!(r.metrics.retries, 1);
        let log = String::from_utf8(trace).unwrap();
        assert!(log.contains("retry-same-candidate Candidate { tree: 1, parent: 0"));
        assert!(log.contains("land Candidate { tree: 1, parent: 0"));
        jobs[0].fails = true;
        jobs[0].flakes = false;
        let r = simulate(jobs, Mode::Pipeline(2), &mut Vec::new()).unwrap();
        assert_eq!(r.jobs[0].state, State::Rejected);
        assert!(matches!(r.jobs[1].state, State::Landed(_)));
        assert!(r.metrics.invalidated >= 2);
    }
    #[test]
    fn batch_failure_splits_and_labels_weaker_evidence() {
        let mut jobs = vec![job(0, Class::Normal), job(1, Class::Normal)];
        let mut trace = Vec::new();
        simulate(jobs.clone(), Mode::Batch(4), &mut trace).unwrap();
        assert!(
            String::from_utf8(trace)
                .unwrap()
                .contains("batch-tip-only;intermediates-not-separately-validated")
        );
        jobs[0].fails = true;
        let r = simulate(jobs, Mode::Batch(4), &mut Vec::new()).unwrap();
        assert_eq!(r.metrics.batch_splits, 1);
        assert_eq!(r.jobs[0].state, State::Rejected);
        assert!(matches!(r.jobs[1].state, State::Landed(_)));
    }
    #[test]
    fn severe_preemption_preserves_submission_and_lease_grace() {
        let mut jobs = vec![
            job(0, Class::Normal),
            job(1, Class::Low),
            job(2, Class::Sev),
        ];
        jobs[2].arrival = 10;
        jobs[2].conflict = true;
        let mut trace = Vec::new();
        let r = simulate(jobs, Mode::Pipeline(2), &mut trace).unwrap();
        assert_eq!(r.metrics.preemptions, 1);
        assert!(r.jobs.iter().all(|j| matches!(j.state, State::Landed(_))));
        let log = String::from_utf8(trace).unwrap();
        assert!(log.contains("t=40 preempt-attempt"));
        assert!(log.contains("members=[2]"));
    }
    #[test]
    fn seeded_replay_is_identical_and_pipeline_can_help_synthetic_load() {
        let jobs = workload(7, 600, 0, false);
        let mut a = Vec::new();
        let mut b = Vec::new();
        let seq = simulate(jobs.clone(), Mode::Sequential, &mut a).unwrap();
        simulate(jobs.clone(), Mode::Sequential, &mut b).unwrap();
        assert_eq!(a, b);
        let pipe = simulate(jobs, Mode::Pipeline(2), &mut Vec::new()).unwrap();
        assert!(pipe.end < seq.end);
        assert!(seq.metrics.pending_at_horizon > 0);
    }
    #[test]
    fn running_inherited_priority_reconciles_severe_cap() {
        let mut jobs = vec![
            job(0, Class::Normal),
            job(1, Class::Sev),
            job(2, Class::Sev),
        ];
        jobs[2].arrival = 10;
        jobs[2].dep = Some(0);
        let mut trace = Vec::new();
        let r = simulate(jobs, Mode::Pipeline(2), &mut trace).unwrap();
        assert!(r.metrics.cap_rebuilds > 0);
        assert!(r.jobs.iter().all(|j| matches!(j.state, State::Landed(_))));
        assert!(String::from_utf8(trace).unwrap().contains("cap-reconcile"));
    }
    #[test]
    fn empty_percentiles_are_missing_not_zero() {
        assert_eq!(percentile(vec![], 95), None);
    }
}
