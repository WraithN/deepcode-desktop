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
    /// View session logs (audit trail)
    Logs {
        #[arg(long, default_value = "50")]
        limit: usize,
        #[arg(long)]
        session_id: Option<String>,
    },
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
        GatewaydCommands::Logs { limit, session_id } => {
            let data_dir = dh_platform::fs::ensure_data_dir()?;
            let db_path = data_dir.join("gatewayd.db");

            if !db_path.exists() {
                println!("No logs found (database does not exist yet)");
                return Ok(());
            }

            let conn = rusqlite::Connection::open(&db_path)?;

            let sql = match &session_id {
                Some(sid) => format!(
                    "SELECT timestamp, direction, provider, model, payload_size_bytes, session_id, request_id \
                     FROM audit_logs WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT {}",
                    limit
                ),
                None => format!(
                    "SELECT timestamp, direction, provider, model, payload_size_bytes, session_id, request_id \
                     FROM audit_logs ORDER BY timestamp DESC LIMIT {}",
                    limit
                ),
            };

            let mut stmt = conn.prepare(&sql)?;

            let rows: Vec<_> = match &session_id {
                Some(sid) => {
                    stmt.query_map([sid.as_str()], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                        ))
                    })?.collect::<Result<Vec<_>, _>>()?
                }
                None => {
                    stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                        ))
                    })?.collect::<Result<Vec<_>, _>>()?
                }
            };

            if rows.is_empty() {
                println!("No audit logs found");
                return Ok(());
            }

            println!("{:<20} {:<10} {:<12} {:<20} {:<10} {:<36} {:<36}",
                "TIMESTAMP", "DIR", "PROVIDER", "MODEL", "SIZE", "SESSION_ID", "REQUEST_ID");
            println!("{}", "-".repeat(150));

            for (timestamp, direction, provider, model, size, sid, rid) in rows {
                println!("{:<20} {:<10} {:<12} {:<20} {:<10} {:<36} {:<36}",
                    &timestamp[..timestamp.len().min(20)],
                    direction,
                    provider,
                    model,
                    size,
                    &sid[..sid.len().min(36)],
                    &rid[..rid.len().min(36)]
                );
            }
        }
    }

    Ok(())
}

async fn check_running() -> bool {
    dh_platform::fs::read_lock_file().ok().flatten().is_some()
}
