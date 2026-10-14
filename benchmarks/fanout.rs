//! Disposable local workload. No results are performance guarantees.
use anyhow::{Result, ensure};
use clap::Parser;
use mai::{core::Repository, protocol::Submission};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 16)]
    agents: usize,
    #[arg(long, default_value_t = 8)]
    workers: usize,
    #[arg(long, default_value_t = 0)]
    hotspot_agents: usize,
    #[arg(long, default_value_t = 0)]
    repository_files: usize,
    /// Exercise native CLI processes instead of in-process core calls.
    #[arg(long)]
    cli: Option<PathBuf>,
}
fn percentile(mut values: Vec<f64>, p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1)])
}
fn run(a: Args) -> Result<bool> {
    ensure!(
        (1..=1024).contains(&a.agents)
            && (1..=1024).contains(&a.workers)
            && a.hotspot_agents <= a.agents,
        "Require agents/workers 1..1024 and hotspot-agents <= agents"
    );
    let temp = tempfile::tempdir()?;
    let r = Repository::init(temp.path().join("project"), None)?;
    if a.repository_files > 0 {
        r.create_workspace("seed")?;
        for i in 0..a.repository_files {
            fs::write(
                r.workspace_path("seed")?.join(format!("seed-{i}")),
                "seed\n",
            )?;
        }
        r.submit("seed", "seed repository", None)?;
    }
    let initial = r.revision()?;
    let setup = Instant::now();
    let mut workspace_ms = Vec::new();
    for i in 0..a.agents {
        let agent = format!("agent-{i:04}");
        let start = Instant::now();
        r.create_workspace(&agent)?;
        workspace_ms.push(start.elapsed().as_secs_f64() * 1000.);
        let p = r.workspace_path(&agent)?.join(if i < a.hotspot_agents {
            "hotspot.txt".into()
        } else {
            format!("agent-{i:04}.txt")
        });
        fs::write(p, format!("written by {agent}\n"))?;
    }
    let setup_seconds = setup.elapsed().as_secs_f64();
    let cold_start = if let Some(cli) = &a.cli {
        let mut times = Vec::new();
        for _ in 0..10 {
            let t = Instant::now();
            ensure!(
                Command::new(cli)
                    .arg("--version")
                    .output()?
                    .status
                    .success(),
                "CLI version failed"
            );
            times.push(t.elapsed().as_secs_f64() * 1000.);
        }
        Some(times)
    } else {
        None
    };
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let start = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..a.workers.min(a.agents) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= a.agents {
                        break;
                    }
                    let agent = format!("agent-{i:04}");
                    let t = Instant::now();
                    let result = if let Some(cli) = &a.cli {
                        (|| -> Result<Submission> {
                            let o = Command::new(cli)
                                .args(["--json", "submit"])
                                .arg(&r.root)
                                .args([&agent, "-m", &format!("change by {agent}")])
                                .output()?;
                            let v: serde_json::Value = serde_json::from_slice(&o.stdout)?;
                            ensure!(
                                o.status.success(),
                                "{}",
                                v["error"].as_str().unwrap_or("CLI failed")
                            );
                            Ok(serde_json::from_value(v["result"].clone())?)
                        })()
                    } else {
                        r.submit(&agent, &format!("change by {agent}"), None)
                    };
                    results.lock().unwrap().push((
                        t.elapsed().as_secs_f64() * 1000.,
                        t.duration_since(start).as_secs_f64() * 1000.,
                        start.elapsed().as_secs_f64() * 1000.,
                        result,
                    ));
                }
            });
        }
    });
    let elapsed = start.elapsed().as_secs_f64();
    let results = results.into_inner().unwrap();
    let mut revisions = Vec::new();
    let mut conflicts = 0;
    let mut errors = Vec::new();
    let mut waits = Vec::new();
    let mut holds = Vec::new();
    let mut retries = 0;
    for (_, _, _, result) in &results {
        match result {
            Ok(s) => {
                revisions.push(s.revision);
                waits.push(s.lock_wait_ms);
                holds.push(s.lock_hold_ms);
                retries += s.retries;
            }
            Err(e) => {
                if e.to_string().contains("overlapping changes on trunk") {
                    conflicts += 1;
                } else {
                    errors.push(e.to_string());
                }
            }
        }
    }
    revisions.sort();
    let expected = a.agents - a.hotspot_agents.saturating_sub(1);
    let final_revision = r.revision()?;
    let mut integrity = errors.is_empty()
        && conflicts == a.agents - expected
        && revisions == (initial + 1..=initial + expected as u64).collect::<Vec<_>>()
        && final_revision == initial + expected as u64;
    let mut publication_integrity = revisions
        == (initial + 1..=initial + revisions.len() as u64).collect::<Vec<_>>()
        && final_revision == initial + revisions.len() as u64;
    for (_, _, _, result) in &results {
        if let Ok(receipt) = result {
            let i = receipt
                .agent
                .strip_prefix("agent-")
                .unwrap()
                .parse::<usize>()?;
            let path = if i < a.hotspot_agents {
                "hotspot.txt".into()
            } else {
                format!("agent-{i:04}.txt")
            };
            publication_integrity &= r
                .show("HEAD", &path)
                .is_ok_and(|bytes| bytes == format!("written by agent-{i:04}\n").as_bytes());
        }
    }
    integrity &= publication_integrity;
    let lat = results.iter().map(|(t, _, _, _)| *t).collect::<Vec<_>>();
    let queue = results.iter().map(|(_, q, _, _)| *q).collect::<Vec<_>>();
    let burst = results.iter().map(|(_, _, b, _)| *b).collect::<Vec<_>>();
    println!(
        "{}",
        json!({"implementation":concat!("mai Rust ", env!("CARGO_PKG_VERSION")),"mode":if a.cli.is_some(){"cli"}else{"core"},"agents":a.agents,"workers":a.workers,"hotspot_agents":a.hotspot_agents,"repository_files":a.repository_files,"accepted":revisions.len(),"conflicts":conflicts,"other_errors":errors,"final_revision":final_revision,"integrity_ok":integrity,"publication_integrity_ok":publication_integrity,"queue_wait_ms_p95":percentile(queue,0.95),"burst_latency_ms_p95":percentile(burst.clone(),0.95),"burst_latency_ms_p99":percentile(burst,0.99),"workspace_setup_seconds":setup_seconds,"workspace_latency_ms_p50":percentile(workspace_ms.clone(),0.5),"workspace_latency_ms_p95":percentile(workspace_ms,0.95),"submit_wall_seconds":elapsed,"accepted_per_second":revisions.len() as f64/elapsed,"submit_latency_ms_p50":percentile(lat.clone(),0.5),"submit_latency_ms_p95":percentile(lat.clone(),0.95),"submit_latency_ms_p99":percentile(lat,0.99),"lock_wait_ms_p95":percentile(waits,0.95),"lock_hold_ms_p95":percentile(holds,0.95),"publication_retries":retries,"cli_cold_start_ms_p50":cold_start.as_ref().and_then(|v|percentile(v.clone(),0.5)),"cli_cold_start_ms_p99":cold_start.and_then(|v|percentile(v,0.99)),"cpu_seconds":null,"peak_rss_kib":null})
    );
    Ok(integrity)
}
fn main() {
    match run(Args::parse()) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
