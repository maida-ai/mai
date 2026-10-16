use clap::Parser;

#[derive(Parser)]
#[command(
    name = "mai",
    version,
    about = "Git-native integration queue: MVP foundations"
)]
struct Cli {}

fn main() {
    let _ = Cli::parse();
}
