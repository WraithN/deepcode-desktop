use clap::{Parser, Subcommand};

mod commands;
mod wrapper;

#[derive(Parser, Debug)]
#[command(name = "dh")]
#[command(about = "DeepHarness CLI - LLM Gateway management and agent wrapper")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Chat with an agent in interactive REPL mode
    Chat(commands::chat::ChatArgs),

    /// Manage configuration and cloud sync
    #[command(subcommand)]
    Config(commands::config::ConfigCommands),

    /// Execute a coding agent with DeepHarness gateway integration
    Exec(commands::exec::ExecArgs),

    /// Manage the gatewayd daemon
    #[command(name = "gwd")]
    Gwd(commands::gatewayd::GwdArgs),

    /// Manage MCP servers and tools
    #[command(subcommand)]
    Mcp(commands::mcp::McpCommands),
}

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        match cli.command {
            Commands::Config(cmd) => {
                commands::config::run(cmd).await
            }
            Commands::Exec(args) => {
                commands::exec::run(args).await
            }
            Commands::Chat(args) => {
                commands::chat::run(args).await
            }
            Commands::Gwd(args) => {
                commands::gatewayd::run(args).await
            }
            Commands::Mcp(cmd) => {
                commands::mcp::run(cmd).await
            }
        }
    });

    if let Err(e) = result {
        eprintln!("dh error: {}", e);
        std::process::exit(1);
    }
}
