use clap::{Parser, Subcommand};
use tracing::{error, info};

use super::gatewayd_support;
use super::detect;

#[derive(Parser, Debug)]
pub struct GwdArgs {
    /// Attach an agent plugin to a running gatewayd (e.g. opencode)
    #[arg(long, global = true)]
    pub attach: Vec<String>,

    #[command(subcommand)]
    pub command: Option<GatewaydCommands>,
}

#[derive(Subcommand, Debug)]
pub enum GatewaydCommands {
    /// Start the gatewayd daemon
    Start {
        #[arg(long)]
        daemon: bool,
        /// Agent types to auto-start (e.g. opencode, claudecode, codex)
        agent_types: Vec<String>,
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
    /// View all requests for a session (prefix match supported, use -1 for latest)
    Session {
        /// Session ID, prefix, or negative index (-1 = latest, -2 = 2nd latest)
        #[arg(allow_hyphen_values = true)]
        session_id: String,
    },
    /// View detailed info for a specific request (prefix match supported, use -1 for latest)
    Request {
        /// Request ID, prefix, or negative index (-1 = latest, -2 = 2nd latest)
        #[arg(allow_hyphen_values = true)]
        request_id: String,
    },
    /// View token usage statistics
    Stats {
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

pub async fn run(args: GwdArgs) -> Result<(), anyhow::Error> {
    // Handle --attach without any subcommand (attach to running gatewayd)
    if args.command.is_none() && !args.attach.is_empty() {
        return gatewayd_support::attach_agents(&args.attach).await;
    }

    let cmd = match args.command {
        Some(c) => c,
        None => anyhow::bail!("No subcommand provided. Use --help for usage."),
    };

    if let GatewaydCommands::Start { daemon, agent_types } = &cmd {
            info!("Starting gatewayd...");

            if !detect::has_any_agent_installed() {
                detect::print_missing_agent_hint();
                anyhow::bail!("Cannot start gatewayd without a coding agent installed");
            }

            if gatewayd_support::check_running().await {
                println!("dh-gatewayd is already running");
                return Ok(());
            }

            let mut cmd = std::process::Command::new("dh-gatewayd");
            if *daemon {
                cmd.arg("--daemon");
            }
            for agent_type in agent_types {
                cmd.arg("--agent-type").arg(agent_type);
            }
            for plugin_type in &args.attach {
                cmd.arg("--attach").arg(plugin_type);
            }

            let mut child = cmd.spawn()?;
            let pid = child.id();
            println!("dh-gatewayd started (PID: {})", pid);

            if !daemon {
                println!("Press Ctrl+C to stop the daemon");
                let status = child.wait()?;
                if !status.success() {
                    error!("dh-gatewayd exited with status: {:?}", status.code());
                }
            }
        }

    if let GatewaydCommands::Stop = &cmd {
            info!("Stopping dh-gatewayd...");

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
                    println!("dh-gatewayd stopped (PID: {})", pid);
                }
                None => {
                    println!("dh-gatewayd is not running");
                }
            }
        }

    if let GatewaydCommands::Status = &cmd {
            match dh_platform::fs::read_lock_file()? {
                Some(pid) if gatewayd_support::is_process_alive(pid) => {
                    let client = reqwest::Client::new();
                    let mut found = false;
                    for admin_port in [2346u16, 2347, 2348, 2349, 2350] {
                        let health_url = format!("http://127.0.0.1:{}/health", admin_port);
                        if let Ok(resp) = client.get(&health_url).timeout(std::time::Duration::from_secs(1)).send().await {
                            if resp.status().is_success() {
                                let body: serde_json::Value = resp.json().await?;
                                let api_port = admin_port - 1;
                                println!("dh-gatewayd is running (PID: {})", pid);
                                println!("  Admin port: {}", admin_port);
                                println!("  API port: {}", api_port);
                                println!("  Version: {}", body.get("version").and_then(|v| v.as_str()).unwrap_or("unknown"));
                                println!();
                                println!("  Endpoints:");
                                println!("    WebSocket: ws://127.0.0.1:{}/agents/events", admin_port);
                                println!("    OpenAI: http://127.0.0.1:{}/v1/chat/completions", api_port);
                                println!("    Anthropic: http://127.0.0.1:{}/v1/messages", api_port);
                                println!();

                                // Show attached agents
                                let agents_url = format!("http://127.0.0.1:{}/agents", admin_port);
                                if let Ok(agents_resp) = client.get(&agents_url).timeout(std::time::Duration::from_secs(2)).send().await {
                                    if agents_resp.status().is_success() {
                                        let agents: Vec<serde_json::Value> = agents_resp.json().await.unwrap_or_default();
                                        if agents.is_empty() {
                                            println!("  Agents: none");
                                        } else {
                                            println!("  Agents ({}):", agents.len());
                                            for agent in agents {
                                                let id = agent.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                let plugin = agent.get("plugin_key").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                let name = agent.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                                                let status = gatewayd_support::format_agent_status(agent.get("status"));
                                                println!("    - {} ({}) [{}]", name, plugin, status);
                                                if let Some(endpoint) = agent.get("endpoint").and_then(|v| v.as_str()) {
                                                    if !endpoint.is_empty() {
                                                        println!("      endpoint: {}", endpoint);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        let _ = dh_platform::fs::remove_lock_file();
                        println!("dh-gatewayd is not running (stale lock file cleaned up)");
                    }
                }
                Some(_) => {
                    let _ = dh_platform::fs::remove_lock_file();
                    println!("dh-gatewayd is not running (stale lock file cleaned up)");
                }
                None => {
                    println!("dh-gatewayd is not running");
                }
            }
        }

    if let GatewaydCommands::Logs { limit, session_id } = &cmd {
            let conn = gatewayd_support::open_db()?;

            let sql = match &session_id {
                Some(_sid) => format!(
                    "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, session_id, request_id \
                     FROM audit_logs WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT {}",
                    limit
                ),
                None => format!(
                    "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, session_id, request_id \
                     FROM audit_logs ORDER BY timestamp DESC LIMIT {}",
                    limit
                ),
            };

            let mut stmt = conn.prepare(&sql)?;

            let rows: Vec<_> = match &session_id {
                Some(sid) => {
                    stmt.query_map([sid.as_str()], gatewayd_support::row_mapper)?.collect::<Result<Vec<_>, _>>()?
                }
                None => {
                    stmt.query_map([], gatewayd_support::row_mapper)?.collect::<Result<Vec<_>, _>>()?
                }
            };

            if rows.is_empty() {
                println!("No audit logs found");
                return Ok(());
            }

            println!("{:<19} {:<10} {:<12} {:<20} {:<12} {:<10} {:<36} {:<36}",
                "TIMESTAMP", "DIR", "PROVIDER", "MODEL", "AGENT", "SIZE", "SESSION_ID", "REQUEST_ID");
            println!("{}", "-".repeat(170));

            for (timestamp, direction, provider, model, agent_type, size, sid, rid) in rows {
                println!("{:<19} {:<10} {:<12} {:<20} {:<12} {:<10} {:<36} {:<36}",
                    gatewayd_support::format_timestamp(&timestamp),
                    direction,
                    provider,
                    model,
                    agent_type.as_deref().unwrap_or("-"),
                    size,
                    &sid[..sid.len().min(36)],
                    &rid[..rid.len().min(36)]
                );
            }
        }

    if let GatewaydCommands::Session { session_id } = &cmd {
            let conn = gatewayd_support::open_db()?;

            let resolved_id = gatewayd_support::resolve_session_index(&conn, session_id)?;
            let target_id = resolved_id.as_ref().unwrap_or(session_id);

            // Try to find session metadata first
            let session_meta = gatewayd_support::find_session(&conn, target_id)?;

            let audit_rows = if let Some(ref meta) = session_meta {
                let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                           FROM audit_logs WHERE session_id = ?1 ORDER BY timestamp ASC";
                let mut stmt = conn.prepare(sql)?;
                let rows: Vec<_> = stmt.query_map([meta.0.as_str()], gatewayd_support::session_row_mapper)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            } else {
                // Fallback: search audit_logs directly
                let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                           FROM audit_logs WHERE session_id LIKE ?1 ORDER BY timestamp ASC";
                let mut stmt = conn.prepare(sql)?;
                let rows: Vec<_> = stmt.query_map([format!("{}%", target_id)], gatewayd_support::session_row_mapper)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };

            if audit_rows.is_empty() && session_meta.is_none() {
                println!("No session found for id: {}", session_id);
                return Ok(());
            }

            gatewayd_support::print_session_details(session_meta.as_ref(), &audit_rows);
        }

    if let GatewaydCommands::Request { request_id } = &cmd {
            let conn = gatewayd_support::open_db()?;

            let resolved_id = gatewayd_support::resolve_request_index(&conn, request_id)?;
            let target_id = resolved_id.as_ref().unwrap_or(request_id);

            let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                       FROM audit_logs WHERE request_id = ?1 ORDER BY timestamp ASC";
            let mut stmt = conn.prepare(sql)?;
            let rows: Vec<_> = stmt.query_map([target_id.as_str()], gatewayd_support::session_row_mapper)?
                .collect::<Result<Vec<_>, _>>()?;

            let rows = if rows.is_empty() {
                let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                           FROM audit_logs WHERE request_id LIKE ?1 ORDER BY timestamp ASC";
                let mut stmt = conn.prepare(sql)?;
                let rows: Vec<_> = stmt.query_map([format!("{}%", target_id)], gatewayd_support::session_row_mapper)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            } else {
                rows
            };

            if rows.is_empty() {
                println!("No request found for id: {}", request_id);
                return Ok(());
            }

            gatewayd_support::print_request_details(&rows);
        }

    if let GatewaydCommands::Stats { session_id, since, provider, model, json } = &cmd {
            let conn = gatewayd_support::open_db()?;
            let mut sql = String::from(
                "SELECT 
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COUNT(CASE WHEN direction = 'request' THEN 1 END),
                    COUNT(CASE WHEN direction = 'response' THEN 1 END),
                    COUNT(CASE WHEN metadata LIKE '%\"token_source\":\"estimated\"%' THEN 1 END)
                 FROM audit_logs WHERE 1=1"
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::new();

            if let Some(ref sid) = session_id {
                sql.push_str(" AND session_id = ?");
                params.push(sid);
            }
            if let Some(ref since_val) = since {
                sql.push_str(" AND timestamp >= ?");
                params.push(since_val);
            }
            if let Some(ref prov) = provider {
                sql.push_str(" AND provider = ?");
                params.push(prov);
            }
            if let Some(ref m) = model {
                sql.push_str(" AND model = ?");
                params.push(m);
            }

            let mut stmt = conn.prepare(&sql)?;
            let (prompt, completion, total, requests, responses, estimated): (
                i64, i64, i64, i64, i64, i64
            ) = stmt.query_row(rusqlite::params_from_iter(params), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?;

            if *json {
                println!("{}", serde_json::json!({
                    "upstream_tokens": prompt,
                    "downstream_tokens": completion,
                    "total_tokens": total,
                    "total_requests": requests,
                    "total_responses": responses,
                    "estimated_count": estimated,
                    "filters": {
                        "session_id": session_id,
                        "since": since,
                        "provider": provider,
                        "model": model,
                    }
                }));
            } else {
                println!("┌──────────────────────┬─────────┬───────────┐");
                println!("│ {:<20} │ {:>7} │ {:<9} │", "Metric", "Value", "Source");
                println!("├──────────────────────┼─────────┼───────────┤");

                let source = if estimated == 0 {
                    "provider"
                } else if estimated >= responses {
                    "estimated"
                } else {
                    "mixed"
                };

                println!("│ {:<20} │ {:>7} │ {:<9} │", "Upstream Tokens", prompt, source);
                println!("│ {:<20} │ {:>7} │ {:<9} │", "Downstream Tokens", completion, source);
                println!("│ {:<20} │ {:>7} │ {:<9} │", "Total Tokens", total, "—");
                println!("│ {:<20} │ {:>7} │ {:<9} │", "Total Requests", requests, "—");
                println!("│ {:<20} │ {:>7} │ {:<9} │", "Total Responses", responses, "—");
                println!("└──────────────────────┴─────────┴───────────┘");

                if estimated > 0 {
                    println!("\n* {} records used estimated token counts", estimated);
                }

                let mut filters = Vec::new();
                if let Some(sid) = &session_id {
                    filters.push(format!("session_id={}", sid));
                }
                if let Some(s) = &since {
                    filters.push(format!("since={}", s));
                }
                if let Some(p) = &provider {
                    filters.push(format!("provider={}", p));
                }
                if let Some(m) = &model {
                    filters.push(format!("model={}", m));
                }
                if !filters.is_empty() {
                    println!("Filters: {}", filters.join(", "));
                }
            }
        }

    Ok(())
}
