use anyhow::Result;
use clap::{Parser, Subcommand};
use mai::{
    authority::Authority,
    engine,
    lifecycle::{ExitCode, SCHEMA_VERSION},
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
    /// Initialize or safely reopen an authority; receive and landing are unavailable.
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

    /// Manually validate exact candidates; receive and landing remain unavailable.
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
enum ValidatorAction {
    /// Run the trusted current-trunk gate on an exact candidate in an isolated slot.
    Run { project: PathBuf, candidate: String },
}
fn run(command: Action) -> Result<Value> {
    Ok(match command {
        Action::Init { project, source } => {
            let authority = Authority::init(&project, source.as_deref())?;
            json!({"schema_version": SCHEMA_VERSION, "authority": NativePath::from_path(authority.root())?, "initialized": true, "registration_available": false})
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
        } => serde_json::to_value(mai::validator::approve(
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
