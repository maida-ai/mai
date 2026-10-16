use anyhow::Result;
use clap::{Parser, Subcommand};
use mai::{
    authority::Authority,
    engine,
    lifecycle::{ExitCode, SCHEMA_VERSION},
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
}
fn run(command: Action) -> Result<Value> {
    Ok(match command {
        Action::Init { project, source } => {
            let authority = Authority::init(&project, source.as_deref())?;
            json!({"schema_version": SCHEMA_VERSION, "authority": authority.root(), "initialized": true, "registration_available": false})
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
    })
}
fn main() {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(Value::Null) => {}
        Ok(value) => {
            if cli.json {
                println!("{value}");
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
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
