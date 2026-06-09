use clap::{Parser, Subcommand};
use tracing::info;

mod commands;
mod wrapper;

#[derive(Parser, Debug)]
#[command(name = "deepharness")]
#[command(about = "DeepHarness CLI - LLM Gateway management and agent wrapper")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute a coding agent with DeepHarness gateway integration
    Exec(commands::exec::ExecArgs),

    /// Manage the gatewayd daemon
    #[command(subcommand)]
    Gatewayd(commands::gatewayd::GatewaydCommands),
}

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        match cli.command {
            Commands::Exec(args) => {
                commands::exec::run(args).await
            }
            Commands::Gatewayd(cmd) => {
                commands::gatewayd::run(cmd).await
            }
        }
    });

    if let Err(e) = result {
        eprintln!("deepharness error: {}", e);
        std::process::exit(1);
    }
}
