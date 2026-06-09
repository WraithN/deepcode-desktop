use clap::Subcommand;
use tracing::{error, info};

#[derive(Subcommand, Debug)]
pub enum GatewaydCommands {
    /// Start the gatewayd daemon
    Start {
        #[arg(long)]
        daemon: bool,
    },
    /// Stop the gatewayd daemon
    Stop,
    /// Check gatewayd status
    Status,
}

pub async fn run(command: GatewaydCommands) -> Result<(), anyhow::Error> {
    match command {
        GatewaydCommands::Start { daemon } => {
            info!("Starting gatewayd...");

            if check_running().await {
                println!("gatewayd is already running");
                return Ok(());
            }

            let mut cmd = std::process::Command::new("gatewayd");
            if daemon {
                cmd.arg("--daemon");
            }

            let mut child = cmd.spawn()?;
            info!("gatewayd started with PID: {}", child.id());

            if !daemon {
                let status = child.wait()?;
                if !status.success() {
                    error!("gatewayd exited with status: {:?}", status.code());
                }
            }

            println!("gatewayd started");
        }
        GatewaydCommands::Stop => {
            info!("Stopping gatewayd...");

            match dh_platform::fs::read_lock_file()? {
                Some(pid) => {
                    #[cfg(unix)]
                    {
                        unsafe {
                            libc::kill(pid as i32, libc::SIGTERM);
                        }
                    }

                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/F"])
                            .output();
                    }

                    dh_platform::fs::remove_lock_file()?;
                    println!("gatewayd stopped (PID: {})", pid);
                }
                None => {
                    println!("gatewayd is not running");
                }
            }
        }
        GatewaydCommands::Status => {
            if check_running().await {
                let client = reqwest::Client::new();
                for port in [2346u16, 2347, 2348, 2349, 2350] {
                    let url = format!("http://127.0.0.1:{}/health", port);
                    if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(1)).send().await {
                        if resp.status().is_success() {
                            let body: serde_json::Value = resp.json().await?;
                            println!("gatewayd is running");
                            println!("  Admin port: {}", port);
                            println!("  Version: {}", body.get("version").and_then(|v| v.as_str()).unwrap_or("unknown"));
                            break;
                        }
                    }
                }
            } else {
                println!("gatewayd is not running");
            }
        }
    }

    Ok(())
}

async fn check_running() -> bool {
    dh_platform::fs::read_lock_file().ok().flatten().is_some()
}
