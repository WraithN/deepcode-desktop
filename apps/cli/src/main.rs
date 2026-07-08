use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

mod commands;
mod wrapper;

// 版本号单一事实来源：根 Cargo.toml 的 [workspace.package] version。
// 通过 #[command(version)] 注入 Command，运行时由 -v / --version（ArgAction::Version）输出。
//
// 说明：clap derive 对显式 ArgAction::Version 的字段会误判为必填参数，
// 因此这里禁用 derive 的 version flag，改为在 main() 中用 builder 手动注册，
// 这样 -v / --version 仍能在解析阶段优先打印版本并退出（无需提供子命令）。
#[derive(Parser, Debug)]
#[command(name = "dh")]
#[command(about = "DeepHarness CLI - LLM Gateway management and agent wrapper")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(disable_version_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Chat with an agent in interactive REPL mode
    Chat(commands::chat::ChatArgs),

    /// Detect installed coding agents
    Detect(commands::detect::DetectArgs),

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

    // 手动注册 -v / --version：ArgAction::Version 在解析阶段优先处理，
    // 出现该 flag 时直接打印版本并退出，不要求提供子命令。
    let cmd = Cli::command().arg(
        Arg::new("version")
            .short('v')
            .long("version")
            .action(ArgAction::Version)
            .help("显示版本号并退出"),
    );
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        match cli.command {
            Commands::Detect(args) => commands::detect::run(args),
            Commands::Config(cmd) => commands::config::run(cmd).await,
            Commands::Exec(args) => commands::exec::run(args).await,
            Commands::Chat(args) => commands::chat::run(args).await,
            Commands::Gwd(args) => commands::gatewayd::run(args).await,
            Commands::Mcp(cmd) => commands::mcp::run(cmd).await,
        }
    });

    if let Err(e) = result {
        eprintln!("dh error: {}", e);
        std::process::exit(1);
    }
}
