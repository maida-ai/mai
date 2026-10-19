use anyhow::Result;
use clap::{Parser, Subcommand};
use mai::{
    authority::Authority,
    engine,
    lifecycle::{ExitCode, GenerationId, SCHEMA_VERSION},
    native_path::NativePath,
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mai",
    version,
    about = "Git-native integration queue: MVP authority foundation"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Validate and integrate one named generation through the serial engine.
    Integrate {
        project: PathBuf,
        change: String,
        #[arg(long, default_value_t = 1)]
        generation: u64,
        #[arg(long, value_parser=["textual"])]
        mode: String,
        #[arg(long="lease", value_parser=parse_claim)]
        claims: Vec<mai::lease::LeaseClaim>,
        #[arg(long, default_value_t = 600)]
        wait_seconds: u64,
    },
    /// Acquire, renew, release or inspect ordinary file leases using engine time.
    Lease {
        #[command(subcommand)]
        command: LeaseCommand,
    },
    /// Configure explicit limits and enable trusted-local Git admission.
    Receive {
        #[command(subcommand)]
        command: ReceiveAction,
    },
    /// Show the durable state of a registered submission.
    Show {
        project: PathBuf,
        change: String,
        #[arg(long, default_value_t = 1)]
        generation: u64,
    },
    /// List durable submission generations in deterministic identity order.
    Queue {
        project: PathBuf,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, value_parser = parse_cursor)]
        after: Option<GenerationId>,
    },
    /// Wait for one exact generation to reach a terminal outcome.
    Wait {
        project: PathBuf,
        change: String,
        #[arg(long, default_value_t = 1)]
        generation: u64,
        #[arg(long, default_value_t = 600)]
        timeout_seconds: u64,
    },
    #[command(hide = true)]
    ReceiveHook { project: PathBuf },
    #[command(hide = true)]
    ReceivePreflight { project: PathBuf },
    /// Initialize or safely reopen an authority; receive is explicitly enabled separately.
    Init {
        project: PathBuf,
        #[arg(long = "from")]
        source: Option<PathBuf>,
    },
    /// Own and reconcile the authority, serving local status and stop requests.
    Serve {
        project: PathBuf,
        #[arg(long, required = true)]
        foreground: bool,
    },
    /// Report engine readiness, starting a local owner unless disabled.
    Status {
        project: PathBuf,
        #[arg(long)]
        no_autostart: bool,
    },
    /// Request graceful engine shutdown without starting one.
    Stop { project: PathBuf },
    /// Create an independent agent clone and run configured bootstrap.
    Workspace {
        project: PathBuf,
        agent: String,
        destination: PathBuf,
        #[arg(long)]
        skip_bootstrap: bool,
    },
    /// Copy a landed submission's clean linear local suffix onto a fresh accepted-trunk workspace.
    Continue {
        project: PathBuf,
        source: PathBuf,
        destination: PathBuf,
        #[arg(long)]
        change: String,
        #[arg(long)]
        generation: u64,
        #[arg(long)]
        skip_bootstrap: bool,
    },

    /// Manually validate exact candidates without registering or landing them.
    Validator {
        #[command(subcommand)]
        command: ValidatorAction,
    },
    /// Record an attributable trusted-local protected-change decision.
    Approve {
        project: PathBuf,
        candidate: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        deny: bool,
    },
}
#[derive(Subcommand)]
enum LeaseCommand {
    Acquire {
        project: PathBuf,
        unit: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        duration_ms: i64,
    },
    Renew {
        project: PathBuf,
        unit: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        generation: u64,
        #[arg(long)]
        duration_ms: i64,
    },
    Release {
        project: PathBuf,
        unit: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        generation: u64,
    },
    Query {
        project: PathBuf,
        unit: String,
    },
}
// Lease claims require a canonical file unit and a positive generation.
fn parse_claim(text: &str) -> std::result::Result<mai::lease::LeaseClaim, String> {
    let (unit, generation) = text
        .rsplit_once('=')
        .ok_or("lease claim must be canonical file:UNIT=GENERATION")?;
    mai::manifest::FileUnit::try_from(unit.to_owned()).map_err(|e| e.to_string())?;
    let generation = generation.parse::<u64>().map_err(|e| e.to_string())?;
    if generation == 0 {
        return Err("lease generation must be positive".into());
    }
    Ok(mai::lease::LeaseClaim {
        unit: unit.into(),
        generation,
    })
}
fn parse_cursor(text: &str) -> std::result::Result<GenerationId, String> {
    let (change_id, generation) = text
        .rsplit_once(':')
        .ok_or("cursor must be CHANGE:GENERATION")?;
    let generation = generation.parse::<u64>().map_err(|e| e.to_string())?;
    if change_id.is_empty() || generation == 0 {
        return Err("cursor must identify a positive generation".into());
    }
    Ok(GenerationId {
        change_id: change_id.into(),
        generation,
    })
}
#[derive(Subcommand)]
enum ReceiveAction {
    /// Enable refs/for/main admission with an explicit maximum input pack size.
    Enable {
        project: PathBuf,
        #[arg(long)]
        max_input_bytes: u64,
        #[arg(long)]
        max_file_bytes: u64,
        #[arg(long)]
        max_changed_files: u64,
        #[arg(long)]
        auto_integrate: bool,
    },
}
#[derive(Subcommand)]
enum ValidatorAction {
    /// Run the trusted current-trunk gate on an exact candidate in an isolated slot.
    Run { project: PathBuf, candidate: String },
    /// Inspect one persistent validator slot and its recovery evidence.
    Inspect {
        project: PathBuf,
        #[arg(long)]
        slot: usize,
    },
    /// Explicitly recover one interrupted slot using its inspected claim ID.
    Recover {
        project: PathBuf,
        #[arg(long)]
        slot: usize,
        #[arg(long)]
        claim: String,
    },
    #[command(hide = true)]
    Worker { project: PathBuf, candidate: String },
}
// Dispatch the selected CLI action and return its JSON result.
fn run(command: Action) -> Result<Value> {
    Ok(match command {
        Action::Integrate {
            project,
            change,
            generation,
            mode,
            claims,
            wait_seconds,
        } => engine::integrate(
            &project,
            GenerationId {
                change_id: change,
                generation,
            },
            mode,
            claims,
            wait_seconds,
        )?,
        Action::Lease { command } => {
            let (project, operation) = match command {
                LeaseCommand::Acquire {
                    project,
                    unit,
                    owner,
                    duration_ms,
                } => (
                    project,
                    engine::LeaseAction::Acquire {
                        unit,
                        owner,
                        duration_ms,
                    },
                ),
                LeaseCommand::Renew {
                    project,
                    unit,
                    owner,
                    generation,
                    duration_ms,
                } => (
                    project,
                    engine::LeaseAction::Renew {
                        owner,
                        claim: mai::lease::LeaseClaim { unit, generation },
                        duration_ms,
                    },
                ),
                LeaseCommand::Release {
                    project,
                    unit,
                    owner,
                    generation,
                } => (
                    project,
                    engine::LeaseAction::Release {
                        owner,
                        claim: mai::lease::LeaseClaim { unit, generation },
                    },
                ),
                LeaseCommand::Query { project, unit } => {
                    (project, engine::LeaseAction::Query { unit })
                }
            };
            engine::lease(&project, operation)?
        }
        Action::Receive {
            command:
                ReceiveAction::Enable {
                    project,
                    max_input_bytes,
                    max_file_bytes,
                    max_changed_files,
                    auto_integrate,
                },
        } => {
            engine::enable_receive(
                &project,
                max_input_bytes,
                max_file_bytes,
                max_changed_files,
                auto_integrate,
            )?;
            json!({"schema_version": SCHEMA_VERSION, "registration_available": true, "max_input_bytes": max_input_bytes, "max_file_bytes": max_file_bytes, "max_changed_files": max_changed_files, "automatic_integration": auto_integrate})
        }
        Action::Show {
            project,
            change,
            generation,
        } => {
            let mut result = serde_json::to_value(engine::show(
                &project,
                GenerationId {
                    change_id: change,
                    generation,
                },
            )?)?;
            result["schema_version"] = json!(SCHEMA_VERSION);
            result
        }
        Action::Queue {
            project,
            limit,
            after,
        } => {
            let mut result = engine::queue(&project, limit, after)?;
            result["schema_version"] = json!(SCHEMA_VERSION);
            result
        }
        Action::Wait {
            project,
            change,
            generation,
            timeout_seconds,
        } => {
            let mut result = engine::wait(
                &project,
                GenerationId {
                    change_id: change,
                    generation,
                },
                std::time::Duration::from_secs(timeout_seconds),
            )?;
            result["schema_version"] = json!(SCHEMA_VERSION);
            if result["kind"] != "pending" && result["state"]["kind"] == "terminal" {
                let outcome = result["state"]["outcome"].clone();
                if outcome["kind"] == "landed" {
                    result["kind"] = json!("landed");
                } else {
                    result["kind"] = json!("rejected");
                    result["reason"] = if outcome["kind"] == "rejected" {
                        outcome["reason"].clone()
                    } else {
                        outcome["kind"].clone()
                    };
                }
            }
            result
        }
        Action::ReceiveHook { project } => {
            mai::receive::hook(&project, false)?;
            Value::Null
        }
        Action::ReceivePreflight { project } => {
            mai::receive::hook(&project, true)?;
            Value::Null
        }
        Action::Init { project, source } => {
            let authority = Authority::init(&project, source.as_deref())?;
            json!({"schema_version": SCHEMA_VERSION, "authority": NativePath::from_path(authority.root())?, "initialized": true, "registration_available": mai::receive::enabled(&authority)?})
        }
        Action::Serve { project, .. } => {
            engine::serve(&project)?;
            Value::Null
        }
        Action::Status {
            project,
            no_autostart,
        } => serde_json::to_value(engine::status(&project, !no_autostart)?)?,
        Action::Stop { project } => serde_json::to_value(engine::stop(&project)?)?,
        Action::Workspace {
            project,
            agent,
            destination,
            skip_bootstrap,
        } => serde_json::to_value(mai::workspace::create(
            &project,
            &agent,
            &destination,
            skip_bootstrap,
        )?)?,
        Action::Continue {
            project,
            source,
            destination,
            change,
            generation,
            skip_bootstrap,
        } => serde_json::to_value(mai::workspace::continue_landed(
            &project,
            &source,
            &destination,
            &change,
            generation,
            skip_bootstrap,
        )?)?,
        Action::Validator {
            command: ValidatorAction::Run { project, candidate },
        } => serde_json::to_value(mai::validator::run(&project, &candidate)?)?,
        Action::Validator {
            command: ValidatorAction::Inspect { project, slot },
        } => serde_json::to_value(engine::validator_inspect(&project, slot)?)?,
        Action::Validator {
            command:
                ValidatorAction::Recover {
                    project,
                    slot,
                    claim,
                },
        } => {
            json!({"schema_version":1,"slot":slot,"claim":claim,"quarantine":NativePath::from_path(&engine::validator_recover(&project, slot, &claim)?)?})
        }
        Action::Validator {
            command: ValidatorAction::Worker { project, candidate },
        } => serde_json::to_value(mai::validator::run_worker(&project, &candidate)?)?,
        Action::Approve {
            project,
            candidate,
            actor,
            reason,
            deny,
        } => serde_json::to_value(engine::approve(
            &project, &candidate, &actor, &reason, deny,
        )?)?,
    })
}
fn main() {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(Value::Null) => {}
        Ok(value) => {
            let exit_code = result_exit_code(&value);
            if cli.json {
                println!("{value}");
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
            if exit_code != ExitCode::Success as i32 {
                std::process::exit(exit_code);
            }
        }
        Err(error) => {
            if cli.json {
                eprintln!(
                    "{}",
                    json!({"schema_version": SCHEMA_VERSION, "error": {"reason": "infrastructure", "message": format!("{error:#}")}})
                );
            } else {
                eprintln!("error: {error:#}");
            }
            std::process::exit(ExitCode::Infrastructure as i32);
        }
    }
}

// Results retain their structured JSON even when the operation is unsuccessful.
fn result_exit_code(value: &Value) -> i32 {
    match value.get("kind").and_then(Value::as_str) {
        Some("pending" | "lease_blocked") => return ExitCode::Pending as i32,
        Some("awaiting_approval") => return ExitCode::Pending as i32,
        Some("rejected") => {
            return if value.get("reason").and_then(Value::as_str) == Some("infrastructure") {
                ExitCode::Infrastructure as i32
            } else {
                ExitCode::Rejected as i32
            };
        }
        _ => {}
    }
    let outcome = value
        .get("outcome")
        .or_else(|| value.get("bootstrap").and_then(|v| v.get("outcome")));
    match outcome.and_then(Value::as_str) {
        Some("approval_required") => ExitCode::Pending as i32,
        Some("policy_denied" | "failure" | "check_failed") => ExitCode::Rejected as i32,
        Some("timeout" | "cancelled" | "cleanup_failure" | "infrastructure" | "policy_changed") => {
            ExitCode::Infrastructure as i32
        }
        _ => ExitCode::Success as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_exit_codes_preserve_lifecycle_failure_categories() {
        use mai::lifecycle::{Diagnostics, Outcome, ReasonCode};
        for reason in [
            ReasonCode::InvalidSubmission,
            ReasonCode::DependencyFailed,
            ReasonCode::Conflict,
            ReasonCode::CheckFailed,
            ReasonCode::PolicyDenied,
            ReasonCode::Infrastructure,
        ] {
            let outcome = Outcome::Rejected {
                reason,
                diagnostics: Diagnostics::default(),
            };
            assert_eq!(
                result_exit_code(&json!({"kind":"rejected", "reason":reason})),
                outcome.exit_code() as i32,
                "reason={reason:?}"
            );
        }
        assert_eq!(result_exit_code(&json!({"kind":"awaiting_approval"})), 6);
        assert_eq!(result_exit_code(&json!({"kind":"lease_blocked"})), 6);
        assert_eq!(result_exit_code(&json!({"kind":"landed"})), 0);
    }
}
