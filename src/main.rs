use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use clap::{Parser, Subcommand};
use mai::core::Repository;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
};
#[derive(Parser)]
#[command(
    name = "mai",
    version,
    about = "Agent-native centralized Git-backed version control"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Init {
        project: PathBuf,
        #[arg(long = "from")]
        source: Option<String>,
    },
    Workspace {
        project: PathBuf,
        agent: String,
    },
    Refresh {
        project: PathBuf,
        agent: String,
        #[arg(long)]
        discard: bool,
    },
    Lease {
        project: PathBuf,
        agent: String,
        #[arg(required=true,num_args=1..)]
        paths: Vec<String>,
        #[arg(long, default_value_t = 900)]
        ttl: u64,
    },
    Release {
        project: PathBuf,
        agent: String,
        paths: Vec<String>,
    },
    Status {
        project: PathBuf,
    },
    Diff {
        project: PathBuf,
        agent: String,
    },
    Submit {
        project: PathBuf,
        agent: String,
        #[arg(short, long)]
        message: String,
        #[arg(long)]
        check: Option<String>,
    },
    History {
        project: PathBuf,
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: u64,
    },
    Show {
        project: PathBuf,
        revision: String,
        path: String,
    },
    GithubLink {
        project: PathBuf,
        url: String,
    },
    GithubSync {
        project: PathBuf,
    },
}
fn run(action: Action, json_output: bool) -> Result<Option<Value>> {
    let value = match action {
        Action::Init { project, source } => {
            let r = Repository::init(project, source.as_deref())?;
            json!({"root":r.root,"revision":0,"head":r.head()?})
        }
        Action::Workspace { project, agent } => {
            Repository::open(project)?.create_workspace(&agent)?
        }
        Action::Refresh {
            project,
            agent,
            discard,
        } => Repository::open(project)?.refresh(&agent, discard)?,
        Action::Lease {
            project,
            agent,
            paths,
            ttl,
        } => Repository::open(project)?.lease(&agent, &paths, ttl)?,
        Action::Release {
            project,
            agent,
            paths,
        } => Repository::open(project)?.release(&agent, &paths)?,
        Action::Status { project } => Repository::open(project)?.status()?,
        Action::Diff { project, agent } => json!(Repository::open(project)?.diff(&agent)?),
        Action::Submit {
            project,
            agent,
            message,
            check,
        } => serde_json::to_value(Repository::open(project)?.submit(
            &agent,
            &message,
            check.as_deref(),
        )?)?,
        Action::History { project, limit } => Repository::open(project)?.history(limit)?,
        Action::Show {
            project,
            revision,
            path,
        } => {
            let bytes = Repository::open(project)?.show(&revision, &path)?;
            if !json_output {
                io::stdout().write_all(&bytes)?;
                return Ok(None);
            }
            json!({"revision":revision,"path":path,"content_base64":STANDARD.encode(bytes)})
        }
        Action::GithubLink { project, url } => Repository::open(project)?.link_github(&url)?,
        Action::GithubSync { project } => Repository::open(project)?.sync_github()?,
    };
    Ok(Some(value))
}
fn main() {
    let json_output = std::env::args().any(|s| s == "--json");
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            if json_output && e.exit_code() != 0 {
                println!("{}", json!({"ok":false,"error":e.to_string()}));
                std::process::exit(2);
            }
            e.exit()
        }
    };
    match run(cli.command, cli.json) {
        Ok(Some(v)) => {
            if cli.json {
                println!("{}", json!({"ok":true,"result":v}));
            } else if let Some(s) = v.as_str() {
                print!("{s}");
                if !s.ends_with('\n') {
                    println!();
                }
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&v).expect("serializable value")
                );
            }
        }
        Ok(None) => {}
        Err(e) => {
            if cli.json {
                println!("{}", json!({"ok":false,"error":e.to_string()}));
            } else {
                eprintln!("mai: {e}");
            }
            std::process::exit(2);
        }
    }
}
