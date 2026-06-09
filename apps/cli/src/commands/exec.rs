use clap::Args;
use tracing::{error, info};

use crate::wrapper::{build_env_map, ProcessManager};

#[derive(Args, Debug)]
pub struct ExecArgs {
    /// The coding agent to execute (e.g., claude, opencode, aider)
    pub agent: String,

    /// Additional arguments to pass to the agent
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub agent_args: Vec<String>,
}

pub async fn run(args: ExecArgs) -> Result<(), anyhow::Error> {
    info!("Executing agent: {} with args: {:?}", args.agent, args.agent_args);

    let gatewayd_info = match check_gatewayd().await {
        Some(info) => info,
        None => {
            info!("gatewayd not running, starting it...");
            start_gatewayd().await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            check_gatewayd().await.ok_or_else(|| anyhow::anyhow!("Failed to start gatewayd"))?
        }
    };

    info!("Using gatewayd at port {}", gatewayd_info.port);

    let env_vars = build_env_map(gatewayd_info.port, gatewayd_info.port + 2);

    let mut child = ProcessManager::spawn_agent(&args.agent, &args.agent_args, &env_vars)?;

    let status = child.wait()?;

    if status.success() {
        info!("Agent exited successfully");
    } else {
        error!("Agent exited with status: {:?}", status.code());
    }

    Ok(())
}

#[derive(Debug)]
struct GatewaydInfo {
    port: u16,
}

async fn check_gatewayd() -> Option<GatewaydInfo> {
    match dh_platform::fs::read_lock_file() {
        Ok(Some(_pid)) => {
            let client = reqwest::Client::new();
            for port in [2345u16, 2346, 2347, 2348, 2349] {
                let admin_port = port + 1;
                let url = format!("http://127.0.0.1:{}/health", admin_port);
                if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(2)).send().await {
                    if resp.status().is_success() {
                        return Some(GatewaydInfo { port });
                    }
                }
            }
            None
        }
        _ => None,
    }
}

async fn start_gatewayd() -> Result<(), anyhow::Error> {
    info!("Starting gatewayd...");

    let mut cmd = std::process::Command::new("gatewayd");
    cmd.arg("--daemon");

    if let Ok(exe_path) = std::env::current_exe() {
        let possible_paths = [
            exe_path.parent().map(|p| p.join("gatewayd")),
            exe_path.parent().and_then(|p| p.parent()).map(|p| p.join("gatewayd")),
        ];
        for path in possible_paths.iter().flatten() {
            if path.exists() {
                cmd = std::process::Command::new(path);
                cmd.arg("--daemon");
                break;
            }
        }
    }

    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    info!("gatewayd started with PID: {}", child.id());
    let _ = child.try_wait();

    Ok(())
}
