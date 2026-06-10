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
            let conn = open_db()?;

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
                    stmt.query_map([sid.as_str()], row_mapper)?.collect::<Result<Vec<_>, _>>()?
                }
                None => {
                    stmt.query_map([], row_mapper)?.collect::<Result<Vec<_>, _>>()?
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
                    format_timestamp(&timestamp),
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
        GatewaydCommands::Session { session_id } => {
            let conn = open_db()?;

            let resolved_id = resolve_session_index(&conn, &session_id)?;
            let target_id = resolved_id.as_ref().unwrap_or(&session_id);

            // Try to find session metadata first
            let session_meta = find_session(&conn, target_id)?;

            let audit_rows = if let Some(ref meta) = session_meta {
                let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                           FROM audit_logs WHERE session_id = ?1 ORDER BY timestamp ASC";
                let mut stmt = conn.prepare(sql)?;
                let rows: Vec<_> = stmt.query_map([meta.0.as_str()], session_row_mapper)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            } else {
                // Fallback: search audit_logs directly
                let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                           FROM audit_logs WHERE session_id LIKE ?1 ORDER BY timestamp ASC";
                let mut stmt = conn.prepare(sql)?;
                let rows: Vec<_> = stmt.query_map([format!("{}%", target_id)], session_row_mapper)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };

            if audit_rows.is_empty() && session_meta.is_none() {
                println!("No session found for id: {}", session_id);
                return Ok(());
            }

            print_session_details(session_meta.as_ref(), &audit_rows);
        }
        GatewaydCommands::Request { request_id } => {
            let conn = open_db()?;

            let resolved_id = resolve_request_index(&conn, &request_id)?;
            let target_id = resolved_id.as_ref().unwrap_or(&request_id);

            let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                       FROM audit_logs WHERE request_id = ?1 ORDER BY timestamp ASC";
            let mut stmt = conn.prepare(sql)?;
            let rows: Vec<_> = stmt.query_map([target_id.as_str()], session_row_mapper)?
                .collect::<Result<Vec<_>, _>>()?;

            let rows = if rows.is_empty() {
                let sql = "SELECT timestamp, direction, provider, model, agent_type, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens, session_id, request_id, metadata, payload \
                           FROM audit_logs WHERE request_id LIKE ?1 ORDER BY timestamp ASC";
                let mut stmt = conn.prepare(sql)?;
                let rows: Vec<_> = stmt.query_map([format!("{}%", target_id)], session_row_mapper)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            } else {
                rows
            };

            if rows.is_empty() {
                println!("No request found for id: {}", request_id);
                return Ok(());
            }

            print_request_details(&rows);
        }
        GatewaydCommands::Stats { session_id, since, provider, model, json } => {
            let conn = open_db()?;
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

            if json {
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
    }

    Ok(())
}

fn open_db() -> Result<rusqlite::Connection, anyhow::Error> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");

    if !db_path.exists() {
        anyhow::bail!("No logs found (database does not exist yet)");
    }

    rusqlite::Connection::open(&db_path).map_err(Into::into)
}

fn format_timestamp(ts: &str) -> String {
    // Convert "2026-06-09T08:36:40.945451827+00:00" -> "2026-06-09 08:36:40"
    if ts.len() >= 19 {
        let mut result = String::with_capacity(19);
        result.push_str(&ts[..10]);  // date
        result.push(' ');
        result.push_str(&ts[11..19]); // time (HH:MM:SS)
        result
    } else {
        ts.to_string()
    }
}

type LogRow = (String, String, String, String, Option<String>, i64, String, String);

fn row_mapper(row: &rusqlite::Row) -> Result<LogRow, rusqlite::Error> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, Option<String>>(4)?,
        row.get::<_, i64>(5)?,
        row.get::<_, String>(6)?,
        row.get::<_, String>(7)?,
    ))
}

type DetailRow = (String, String, String, String, Option<String>, i64, Option<i64>, Option<i64>, Option<i64>, String, String, String, Option<String>);

fn session_row_mapper(row: &rusqlite::Row) -> Result<DetailRow, rusqlite::Error> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, Option<String>>(4)?,
        row.get::<_, i64>(5)?,
        row.get::<_, Option<i64>>(6)?,
        row.get::<_, Option<i64>>(7)?,
        row.get::<_, Option<i64>>(8)?,
        row.get::<_, String>(9)?,
        row.get::<_, String>(10)?,
        row.get::<_, String>(11)?,
        row.get::<_, Option<String>>(12)?,
    ))
}

type SessionMeta = (String, String, String, Option<String>, String, String, String);

fn find_session(conn: &rusqlite::Connection, prefix: &str) -> Result<Option<SessionMeta>, rusqlite::Error> {
    // Try exact match
    let sql = "SELECT id, agent_type, model, workspace, started_at, last_active_at, status FROM sessions WHERE id = ?1";
    let mut stmt = conn.prepare(sql)?;
    let mut rows: Vec<_> = stmt.query_map([prefix], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?.collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        // Try prefix match
        let sql = "SELECT id, agent_type, model, workspace, started_at, last_active_at, status FROM sessions WHERE id LIKE ?1 LIMIT 1";
        let mut stmt = conn.prepare(sql)?;
        rows = stmt.query_map([format!("{}%", prefix)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?.collect::<Result<Vec<_>, _>>()?;
    }

    Ok(rows.into_iter().next())
}

fn print_session_details(meta: Option<&SessionMeta>, rows: &[DetailRow]) {
    let sid = meta.map(|m| m.0.as_str()).unwrap_or_else(|| {
        rows.first().map(|r| r.9.as_str()).unwrap_or("unknown")
    });

    println!("═ Session: {} ═", sid);

    if let Some((_, agent_type, model, workspace, started_at, last_active_at, status)) = meta {
        println!("  Agent:       {}", agent_type);
        println!("  Model:       {}", model);
        if let Some(ws) = workspace {
            if !ws.is_empty() {
                println!("  Workspace:   {}", ws);
            }
        }
        println!("  Started:     {}", format_timestamp(started_at));
        println!("  Last active: {}", format_timestamp(last_active_at));
        println!("  Status:      {}", status);
    }

    println!("  Requests: {}", rows.len());
    println!("");

    println!("{:<19} {:<10} {:<12} {:<20} {:<12} {:<10} {:<10} {:<10} {:<36}",
        "TIMESTAMP", "DIR", "PROVIDER", "MODEL", "AGENT", "SIZE", "PROMPT", "COMPLETE", "REQUEST_ID");
    println!("{}", "-".repeat(160));

    for (timestamp, direction, provider, model, agent_type, size, prompt, complete, _total, _sid, rid, _meta, _payload) in rows {
        println!("{:<19} {:<10} {:<12} {:<20} {:<12} {:<10} {:<10} {:<10} {:<36}",
            format_timestamp(timestamp),
            direction,
            provider,
            model,
            agent_type.as_deref().unwrap_or("-"),
            size,
            prompt.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            complete.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            &rid[..rid.len().min(36)]
        );
    }
}

fn print_request_details(rows: &[DetailRow]) {
    for (timestamp, direction, provider, model, agent_type, size, prompt, complete, total, sid, rid, meta, payload) in rows {
        println!("═ Request: {} ═", rid);
        println!("  Timestamp:   {}", format_timestamp(timestamp));
        println!("  Direction:   {}", direction);
        println!("  Session:     {}", sid);
        println!("  Provider:    {}", provider);
        println!("  Model:       {}", model);
        println!("  Agent:       {}", agent_type.as_deref().unwrap_or("-"));
        println!("  Size:        {} bytes", size);
        if let Some(p) = prompt {
            println!("  Prompt tok:  {}", p);
        }
        if let Some(c) = complete {
            println!("  Complete tok: {}", c);
        }
        if let Some(t) = total {
            println!("  Total tok:   {}", t);
        }
        if let Some(p) = payload {
            if !p.is_empty() {
                println!("  Payload:");
                // Pretty print JSON if possible
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(p) {
                    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_else(|_| p.clone()));
                } else {
                    for line in p.lines().take(50) {
                        println!("    {}", line);
                    }
                }
            }
        }
        if meta != "{}" && meta != "null" {
            println!("  Metadata:    {}", meta);
        }
        println!("");
    }
}

fn resolve_session_index(conn: &rusqlite::Connection, input: &str) -> Result<Option<String>, rusqlite::Error> {
    if let Some(n_str) = input.strip_prefix('-') {
        if let Ok(n) = n_str.parse::<usize>() {
            if n == 0 {
                return Ok(None);
            }
            let sql = "SELECT id FROM sessions ORDER BY last_active_at DESC LIMIT 1 OFFSET ?1";
            let mut stmt = conn.prepare(sql)?;
            let rows: Vec<String> = stmt.query_map([n - 1], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(rows.into_iter().next());
        }
    }
    Ok(None)
}

fn resolve_request_index(conn: &rusqlite::Connection, input: &str) -> Result<Option<String>, rusqlite::Error> {
    if let Some(n_str) = input.strip_prefix('-') {
        if let Ok(n) = n_str.parse::<usize>() {
            if n == 0 {
                return Ok(None);
            }
            let sql = "SELECT request_id FROM audit_logs WHERE direction = 'request' ORDER BY timestamp DESC LIMIT 1 OFFSET ?1";
            let mut stmt = conn.prepare(sql)?;
            let rows: Vec<String> = stmt.query_map([n - 1], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(rows.into_iter().next());
        }
    }
    Ok(None)
}

async fn check_running() -> bool {
    dh_platform::fs::read_lock_file().ok().flatten().is_some()
}
