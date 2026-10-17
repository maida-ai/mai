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
    },
}
#[derive(Subcommand)]
enum ValidatorAction {
    /// Run the trusted current-trunk gate on an exact candidate in an isolated slot.
    Run { project: PathBuf, candidate: String },
}
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
                },
        } => {
            engine::enable_receive(&project, max_input_bytes, max_file_bytes, max_changed_files)?;
            json!({"schema_version": SCHEMA_VERSION, "registration_available": true, "max_input_bytes": max_input_bytes, "max_file_bytes": max_file_bytes, "max_changed_files": max_changed_files, "automatic_integration": false})
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
        Action::Validator {
            command: ValidatorAction::Run { project, candidate },
        } => serde_json::to_value(mai::validator::run(&project, &candidate)?)?,
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
        Some("awaiting_approval") => return ExitCode::Pending as i32,
        Some("rejected") => return ExitCode::Rejected as i32,
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
