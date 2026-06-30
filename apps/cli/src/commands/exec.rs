use clap::Args;
use tracing::{error, info, warn};

use crate::wrapper::{build_env_map, ConfigInterceptor, ProcessManager};

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

    // Generate a persistent session ID for this agent run
    let session_id = uuid::Uuid::new_v4().to_string();
    std::env::set_var("DEEPHARNESS_SESSION_ID", &session_id);
    info!("Session ID: {}", session_id);

    let workspace = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));

    let gatewayd_info = match check_gatewayd().await {
        Some(_info) => {
            // Always restart gatewayd to pick up latest API keys from config
            info!("Restarting gatewayd to inject latest API keys...");
            terminate_gatewayd().await;
            start_gatewayd().await?;
            wait_for_gatewayd().await.ok_or_else(|| anyhow::anyhow!("Failed to start gatewayd"))?
        }
        None => {
            info!("gatewayd not running, starting it...");
            start_gatewayd().await?;
            wait_for_gatewayd().await.ok_or_else(|| anyhow::anyhow!("Failed to start gatewayd"))?
        }
    };

    info!("Using gatewayd at port {}", gatewayd_info.port);

    // Register agent type and session with gatewayd for audit logging
    let admin_port = gatewayd_info.port + 1;
    let context_url = format!("http://127.0.0.1:{}/context", admin_port);
    let client = reqwest::Client::new();
    let model = read_opencode_model(&args.agent);
    let _ = client
        .post(&context_url)
        .json(&serde_json::json!({
            "agent_type": args.agent,
            "session_id": session_id,
            "workspace": workspace,
            "model": model
        }))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    let env_vars = build_env_map(gatewayd_info.port, gatewayd_info.port + 2);

    // Intercept agent config to route LLM requests through gatewayd
    let gatewayd_url = format!("http://127.0.0.1:{}/v1", gatewayd_info.port);
    let mut interceptor = ConfigInterceptor::new(&args.agent, &gatewayd_url);
    match interceptor.intercept() {
        Ok(true) => info!("Agent config intercepted successfully"),
        Ok(false) => info!("No config interception needed for agent: {}", args.agent),
        Err(e) => warn!("Failed to intercept agent config: {}", e),
    }

    let mut child = ProcessManager::spawn_agent(&args.agent, &args.agent_args, &env_vars)?;

    let status = child.wait()?;

    // Restore original config before checking exit status
    interceptor.restore();

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
                if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_millis(500)).send().await {
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

async fn wait_for_gatewayd() -> Option<GatewaydInfo> {
    // Poll every 100ms for up to 2 seconds
    for _ in 0..20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Some(info) = check_gatewayd().await {
            return Some(info);
        }
    }
    None
}

#[cfg(unix)]
async fn terminate_gatewayd() {
    if let Ok(Some(pid)) = dh_platform::fs::read_lock_file() {
        let _ = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        // Poll until old gatewayd is gone (max 1s)
        for _ in 0..10 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            if check_gatewayd().await.is_none() {
                break;
            }
        }
    }
}

#[cfg(windows)]
async fn terminate_gatewayd() {
    // Graceful restart via PID termination is not yet implemented on Windows.
    warn!("Graceful gatewayd restart is not yet implemented on Windows");
}

fn read_opencode_model(agent: &str) -> Option<String> {
    if agent != "opencode" {
        return None;
    }
    let config_path = dirs::home_dir()?.join(".config/opencode/opencode.json");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    config.get("model")?.as_str().map(|s| s.to_string())
}

fn read_opencode_api_key(provider: &str) -> Option<String> {
    let config_path = dirs::home_dir()?.join(".config/opencode/opencode.json");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    config.get("provider")?
        .get(provider)?
        .get("options")?
        .get("apiKey")?
        .as_str()
        .map(|s| s.to_string())
}

pub(crate) async fn start_gatewayd() -> Result<(), anyhow::Error> {
    info!("Starting gatewayd...");

    // Inject session ID and API keys from opencode config into gatewayd environment
    if let Ok(session_id) = std::env::var("DEEPHARNESS_SESSION_ID") {
        std::env::set_var("DEEPHARNESS_SESSION_ID", session_id);
    }
    if let Some(key) = read_opencode_api_key("deepseek") {
        std::env::set_var("DEEPSEEK_API_KEY", key);
        info!("Injected DEEPSEEK_API_KEY from opencode config");
    }
    if let Some(key) = read_opencode_api_key("openai") {
        std::env::set_var("OPENAI_API_KEY", key);
        info!("Injected OPENAI_API_KEY from opencode config");
    }

    let mut cmd = std::process::Command::new("dh-gatewayd");
    cmd.arg("--daemon");

    if let Ok(exe_path) = std::env::current_exe() {
        let possible_paths = [
            exe_path.parent().map(|p| p.join("dh-gatewayd")),
            exe_path.parent().and_then(|p| p.parent()).map(|p| p.join("dh-gatewayd")),
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
