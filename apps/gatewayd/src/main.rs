use dh_gatewayd::Args;
use clap::Parser;
use tracing::info;

fn main() {
    // tracing-subscriber 开启 tracing-log feature 后会自动桥接 log crate，
    // 使 claude-plugin 等依赖 log 的 crate 也能输出到同一 subscriber。
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    if !args.agent_types.is_empty() {
        info!("Auto-start agents: {:?}", args.agent_types);
    }
    info!(
        "Starting gatewayd on port {}, admin on port {}",
        args.port, args.admin_port
    );

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    if let Err(e) = rt.block_on(dh_gatewayd::server::run(args)) {
        eprintln!("gatewayd error: {}", e);
        std::process::exit(1);
    }
}
