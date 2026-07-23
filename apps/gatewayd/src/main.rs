use dh_gatewayd::Args;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// 未设置 RUST_LOG 时的默认日志级别。空 EnvFilter 默认只放行 ERROR，
/// 会导致线上日志完全静默（0 字节），因此显式回退到 info。
const DEFAULT_LOG_FILTER: &str = "info";

fn main() {
    // tracing-subscriber 开启 tracing-log feature 后会自动桥接 log crate，
    // 使 claude-plugin 等依赖 log 的 crate 也能输出到同一 subscriber。
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));
    tracing_subscriber::fmt().with_env_filter(filter).init();
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
