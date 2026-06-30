
pub type LogRow = (String, String, String, String, Option<String>, i64, String, String);

pub type DetailRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    String,
    String,
    String,
    Option<String>,
);

pub type SessionMeta = (String, String, String, Option<String>, String, String, String);

pub async fn attach_agents(plugins: &[String]) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let admin_port = find_admin_port(&client).await?;
    let base_url = format!("http://127.0.0.1:{}", admin_port);

    for plugin_type in plugins {
        let url = format!("{}/agents", base_url);
        let workspace = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let payload = serde_json::json!({
            "plugin_type": plugin_type,
            "name": format!("{}-attached", plugin_type),
            "workspace": workspace,
        });

        match client
            .post(&url)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await?;
                if let Some(id) = body.get("instance_id").and_then(|v| v.as_str()) {
                    println!("Attached agent: {} (id={})", plugin_type, id);
                } else {
                    println!("Attached agent: {}", plugin_type);
                }
            }
            Ok(resp) => {
                eprintln!("Failed to attach agent {}: {}", plugin_type, resp.text().await?);
            }
            Err(e) => {
                eprintln!("Failed to attach agent {}: {}", plugin_type, e);
            }
        }
    }
    Ok(())
}

pub async fn find_admin_port(client: &reqwest::Client) -> Result<u16, anyhow::Error> {
    for port in [2346u16, 2347, 2348, 2349, 2350] {
        let url = format!("http://127.0.0.1:{}/health", port);
        if let Ok(resp) = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(1))
            .send()
            .await
        {
            if resp.status().is_success() {
                return Ok(port);
            }
        }
    }
    anyhow::bail!("dh-gatewayd is not running on any known admin port")
}

pub fn format_agent_status(status: Option<&serde_json::Value>) -> String {
    match status {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(map)) => {
            if map.contains_key("running") {
                "running".to_string()
            } else if let Some(serde_json::Value::String(msg)) = map.get("crashed") {
                format!("crashed: {}", msg)
            } else {
                "unknown".to_string()
            }
        }
        _ => "unknown".to_string(),
    }
}

pub fn open_db() -> Result<rusqlite::Connection, anyhow::Error> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");

    if !db_path.exists() {
        anyhow::bail!("No logs found (database does not exist yet)");
    }

    rusqlite::Connection::open(&db_path).map_err(Into::into)
}

pub fn format_timestamp(ts: &str) -> String {
    // Convert "2026-06-09T08:36:40.945451827+00:00" -> "2026-06-09 08:36:40"
    if ts.len() >= 19 {
        let mut result = String::with_capacity(19);
        result.push_str(&ts[..10]); // date
        result.push(' ');
        result.push_str(&ts[11..19]); // time (HH:MM:SS)
        result
    } else {
        ts.to_string()
    }
}

pub fn row_mapper(row: &rusqlite::Row) -> Result<LogRow, rusqlite::Error> {
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

pub fn session_row_mapper(row: &rusqlite::Row) -> Result<DetailRow, rusqlite::Error> {
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

pub fn find_session(
    conn: &rusqlite::Connection,
    prefix: &str,
) -> Result<Option<SessionMeta>, rusqlite::Error> {
    // Try exact match
    let sql =
        "SELECT id, agent_type, model, workspace, started_at, last_active_at, status FROM sessions WHERE id = ?1";
    let mut stmt = conn.prepare(sql)?;
    let mut rows: Vec<_> = stmt
        .query_map([prefix], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        // Try prefix match
        let sql = "SELECT id, agent_type, model, workspace, started_at, last_active_at, status FROM sessions WHERE id LIKE ?1 LIMIT 1";
        let mut stmt = conn.prepare(sql)?;
        rows = stmt
            .query_map([format!("{}%", prefix)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }

    Ok(rows.into_iter().next())
}

pub fn print_session_details(meta: Option<&SessionMeta>, rows: &[DetailRow]) {
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
    println!();

    println!(
        "{:<19} {:<10} {:<12} {:<20} {:<12} {:<10} {:<10} {:<10} {:<36}",
        "TIMESTAMP",
        "DIR",
        "PROVIDER",
        "MODEL",
        "AGENT",
        "SIZE",
        "PROMPT",
        "COMPLETE",
        "REQUEST_ID"
    );
    println!("{}", "-".repeat(160));

    for (
        timestamp,
        direction,
        provider,
        model,
        agent_type,
        size,
        prompt,
        complete,
        _total,
        _sid,
        rid,
        _meta,
        _payload,
    ) in rows
    {
        println!(
            "{:<19} {:<10} {:<12} {:<20} {:<12} {:<10} {:<10} {:<10} {:<36}",
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

pub fn print_request_details(rows: &[DetailRow]) {
    for (
        timestamp,
        direction,
        provider,
        model,
        agent_type,
        size,
        prompt,
        complete,
        total,
        sid,
        rid,
        meta,
        payload,
    ) in rows
    {
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
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json).unwrap_or_else(|_| p.clone())
                    );
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
        println!();
    }
}

pub fn resolve_session_index(
    conn: &rusqlite::Connection,
    input: &str,
) -> Result<Option<String>, rusqlite::Error> {
    if let Some(n_str) = input.strip_prefix('-') {
        if let Ok(n) = n_str.parse::<usize>() {
            if n == 0 {
                return Ok(None);
            }
            let sql = "SELECT id FROM sessions ORDER BY last_active_at DESC LIMIT 1 OFFSET ?1";
            let mut stmt = conn.prepare(sql)?;
            let rows: Vec<String> = stmt
                .query_map([n - 1], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(rows.into_iter().next());
        }
    }
    Ok(None)
}

pub fn resolve_request_index(
    conn: &rusqlite::Connection,
    input: &str,
) -> Result<Option<String>, rusqlite::Error> {
    if let Some(n_str) = input.strip_prefix('-') {
        if let Ok(n) = n_str.parse::<usize>() {
            if n == 0 {
                return Ok(None);
            }
            let sql = "SELECT request_id FROM audit_logs WHERE direction = 'request' ORDER BY timestamp DESC LIMIT 1 OFFSET ?1";
            let mut stmt = conn.prepare(sql)?;
            let rows: Vec<String> = stmt
                .query_map([n - 1], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(rows.into_iter().next());
        }
    }
    Ok(None)
}

pub async fn check_running() -> bool {
    match dh_platform::fs::read_lock_file() {
        Ok(Some(pid)) => is_process_alive(pid),
        _ => false,
    }
}

/// Verify whether a process with the given PID is alive.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        if libc::kill(pid as i32, 0) == 0 {
            return true;
        }
        // kill returns -1 on error. EPERM means the process exists but we lack
        // permission to signal it, so it is still considered alive. Using
        // std::io::Error avoids platform-specific errno accessors.
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(windows)]
pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        let handle = windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_QUERY_INFORMATION,
            0,
            pid,
        );
        if handle == 0 {
            return false;
        }
        let mut code: u32 = 0;
        let ok = windows_sys::Win32::System::Threading::GetExitCodeProcess(handle as _, &mut code);
        windows_sys::Win32::Foundation::CloseHandle(handle as _);
        // STILL_ACTIVE is defined as 259 in winnt.h.
        ok != 0 && code == 259
    }
}
