use clap::Subcommand;
use serde_json::{json, Value};
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// List all MCP servers and their status
    List,
    /// Add a new MCP server
    Add {
        /// Server name (used as namespace)
        name: String,
        /// Command to spawn the MCP server (e.g., npx, uvx)
        #[arg(long)]
        cmd: String,
        /// Command arguments (comma-separated, use -- to pass flags)
        #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
        args: Vec<String>,
        /// Environment variables (KEY=VAL, comma-separated)
        #[arg(long, value_delimiter = ',')]
        env: Vec<String>,
    },
    /// Remove an MCP server
    Remove {
        /// Server name
        name: String,
    },
    /// Call an MCP tool
    Call {
        /// Full tool name with namespace (e.g., filesystem:read_file)
        tool: String,
        /// Tool arguments as JSON string
        #[arg(long, default_value = "{}")]
        args: String,
    },
}

pub async fn run(command: McpCommands) -> Result<(), anyhow::Error> {
    match command {
        McpCommands::List => {
            match list_via_api().await {
                Ok(()) => {}
                Err(e) => {
                    info!("Gatewayd API unavailable ({}), falling back to DB", e);
                    list_via_db()?;
                }
            }
        }
        McpCommands::Add { name, cmd, args, env } => {
            let conn = open_db()?;

            let env_map: std::collections::HashMap<String, String> = env
                .into_iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    let key = parts.next()?;
                    let val = parts.next()?;
                    Some((key.to_string(), val.to_string()))
                })
                .collect();

            let args_json = serde_json::to_string(&args)?;
            let env_json = serde_json::to_string(&env_map)?;

            conn.execute(
                "INSERT OR REPLACE INTO mcp_servers (name, command, args, env, enabled, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, 1, datetime('now'), datetime('now'))",
                rusqlite::params![&name, &cmd, &args_json, &env_json],
            )?;

            println!("Added MCP server: {}", name);
            println!("  Command: {} {:?}", cmd, args);
            println!("  Restart dh-gatewayd to apply changes.");
        }
        McpCommands::Remove { name } => {
            let conn = open_db()?;
            let affected = conn.execute(
                "DELETE FROM mcp_servers WHERE name = ?1",
                [&name],
            )?;

            if affected == 0 {
                println!("No MCP server found: {}", name);
            } else {
                println!("Removed MCP server: {}", name);
                println!("  Restart dh-gatewayd to apply changes.");
            }
        }
        McpCommands::Call { tool, args } => {
            let arguments: Value = serde_json::from_str(&args)
                .map_err(|e| anyhow::anyhow!("Invalid JSON arguments: {}", e))?;

            let client = reqwest::Client::new();
            for port in [2346u16, 2347, 2348, 2349, 2350] {
                let url = format!("http://127.0.0.1:{}/mcp/tools/{}/call", port, tool);
                match client
                    .post(&url)
                    .json(&json!({ "arguments": arguments }))
                    .timeout(std::time::Duration::from_secs(30))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            let body: Value = resp.json().await?;
                            println!("{}", serde_json::to_string_pretty(&body)?);
                            return Ok(());
                        }
                        if resp.status().as_u16() == 503 {
                            anyhow::bail!("MCP aggregator not available (disabled or no servers configured)");
                        }
                    }
                    Err(_) => continue,
                }
            }
            anyhow::bail!("dh-gatewayd is not running or MCP endpoint unavailable");
        }
    }

    Ok(())
}

async fn list_via_api() -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    for port in [2346u16, 2347, 2348, 2349, 2350] {
        let url = format!("http://127.0.0.1:{}/mcp/servers", port);
        match client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body: Value = resp.json().await?;
                    let servers = body.get("servers").and_then(|v| v.as_array()).cloned().unwrap_or_default();

                    if servers.is_empty() {
                        println!("No MCP servers configured.");
                        return Ok(());
                    }

                    // Fetch tools to show count per server
                    let tools_url = format!("http://127.0.0.1:{}/mcp/tools", port);
                    let mut tools_by_server: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    if let Ok(tools_resp) = client.get(&tools_url).timeout(std::time::Duration::from_secs(2)).send().await {
                        if let Ok(tools_body) = tools_resp.json::<Value>().await {
                            if let Some(tools) = tools_body.get("tools").and_then(|v| v.as_array()) {
                                for tool in tools {
                                    if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
                                        if let Some(ns) = name.split(':').next() {
                                            *tools_by_server.entry(ns.to_string()).or_insert(0) += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    println!("{:<20} {:<10} {:<20}", "NAME", "STATUS", "TOOLS");
                    println!("{}", "-".repeat(55));

                    for server in servers {
                        let name = server.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let alive = server.get("alive").and_then(|v| v.as_bool()).unwrap_or(false);
                        let status = if alive { "alive" } else { "dead" };
                        let tool_count = tools_by_server.get(name).copied().unwrap_or(0);
                        println!("{:<20} {:<10} {:<20}", name, status, format!("{} tools", tool_count));
                    }
                    return Ok(());
                }
            }
            Err(_) => continue,
        }
    }
    anyhow::bail!("dh-gatewayd API not available")
}

fn list_via_db() -> Result<(), anyhow::Error> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT name, command, args, enabled FROM mcp_servers ORDER BY name"
    )?;
    let rows: Vec<(String, String, String, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        println!("No MCP servers configured.");
        return Ok(());
    }

    println!("{:<20} {:<10} {:<30} {:<10}", "NAME", "STATUS", "COMMAND", "ENABLED");
    println!("{}", "-".repeat(75));

    for (name, cmd, args, enabled) in rows {
        let args_parsed: Vec<String> = serde_json::from_str(&args).unwrap_or_default();
        let cmd_display = format!("{} {}", cmd, args_parsed.join(" "));
        let enabled_str = if enabled != 0 { "yes" } else { "no" };
        let cmd_truncated = if cmd_display.len() > 29 {
            format!("{}...", &cmd_display[..26])
        } else {
            cmd_display
        };
        println!("{:<20} {:<10} {:<30} {:<10}",
            name,
            "unknown",
            cmd_truncated,
            enabled_str
        );
    }

    println!("\nNote: dh-gatewayd is not running. Status shows 'unknown'.");
    Ok(())
}

fn open_db() -> Result<rusqlite::Connection, anyhow::Error> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");

    if !db_path.exists() {
        let _ = dh_db::DbManager::open(&db_path)?;
    }

    rusqlite::Connection::open(&db_path).map_err(Into::into)
}
