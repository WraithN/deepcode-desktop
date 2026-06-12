use clap::Subcommand;
use tracing::{error, info};

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Set a configuration value
    Set {
        /// Config key (e.g., remote-url, refresh-time)
        key: String,
        /// Config value
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Config key
        key: String,
    },
    /// List all configuration values
    List,
    /// Refresh data from cloud
    #[command(subcommand)]
    Refresh(RefreshCommands),
}

#[derive(Subcommand, Debug)]
pub enum RefreshCommands {
    /// Sync engineering rules/specs from cloud
    Rules,
    /// Sync skills from cloud
    Skills,
}

// Config keys stored in the database
const KEY_REMOTE_URL: &str = "remote_url";
const KEY_REFRESH_TIME: &str = "refresh_time";
const KEY_RULES_DATA: &str = "rules_data";
const KEY_SKILLS_DATA: &str = "skills_data";

// Default refresh time in seconds
const DEFAULT_REFRESH_TIME: &str = "60";

pub async fn run(command: ConfigCommands) -> Result<(), anyhow::Error> {
    match command {
        ConfigCommands::Set { key, value } => {
            let conn = open_db()?;
            let db_key = normalize_key(&key);

            conn.execute(
                "INSERT OR REPLACE INTO configs (key, value, updated_at) VALUES (?1, ?2, datetime('now'))",
                [&db_key, &value],
            )?;

            println!("Set config: {} = {}", key, value);
            info!("Config set: {} = {}", db_key, value);
        }
        ConfigCommands::Get { key } => {
            let conn = open_db()?;
            let db_key = normalize_key(&key);

            match get_config_value(&conn, &db_key)? {
                Some(value) => println!("{} = {}", key, value),
                None => println!("Config '{}' not found", key),
            }
        }
        ConfigCommands::List => {
            let conn = open_db()?;

            let mut stmt = conn.prepare(
                "SELECT key, value, updated_at FROM configs ORDER BY key"
            )?;
            let rows: Vec<(String, String, String)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            if rows.is_empty() {
                println!("No config entries found.");
                println!("  remote_url:    not set");
                println!("  refresh_time:  {}s (default)", DEFAULT_REFRESH_TIME);
                return Ok(());
            }

            println!("{:<20} {:<40} {:<20}", "KEY", "VALUE", "UPDATED_AT");
            println!("{}", "-".repeat(85));

            let mut has_remote_url = false;
            let mut has_refresh_time = false;

            for (key, value, updated_at) in &rows {
                if key == KEY_REMOTE_URL {
                    has_remote_url = true;
                }
                if key == KEY_REFRESH_TIME {
                    has_refresh_time = true;
                }
                println!("{:<20} {:<40} {:<20}", key, truncate(value, 38), updated_at);
            }

            if !has_remote_url {
                println!("{:<20} {:<40} {:<20}", KEY_REMOTE_URL, "(not set)", "-");
            }
            if !has_refresh_time {
                println!("{:<20} {:<40} {:<20}", KEY_REFRESH_TIME, format!("{}s (default)", DEFAULT_REFRESH_TIME), "-");
            }
        }
        ConfigCommands::Refresh(cmd) => match cmd {
            RefreshCommands::Rules => {
                refresh_from_cloud(KEY_RULES_DATA, "/api/rules", "rules").await?;
            }
            RefreshCommands::Skills => {
                refresh_from_cloud(KEY_SKILLS_DATA, "/api/skills", "skills").await?;
            }
        },
    }

    Ok(())
}

async fn refresh_from_cloud(
    config_key: &str,
    api_path: &str,
    label: &str,
) -> Result<(), anyhow::Error> {
    let conn = open_db()?;

    let remote_url = match get_config_value(&conn, KEY_REMOTE_URL)? {
        Some(url) => url,
        None => {
            anyhow::bail!(
                "Cloud URL not configured. Run: dh config set remote-url <url>"
            );
        }
    };

    let url = format!("{}{}", remote_url.trim_end_matches('/'), api_path);
    info!("Refreshing {} from {}", label, url);
    println!("Syncing {} from {} ...", label, url);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;

    match resp {
        Ok(response) => {
            if !response.status().is_success() {
                anyhow::bail!(
                    "Server returned error: {} {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
            }

            let body = response.text().await?;
            // Validate it's valid JSON
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&body) {
                anyhow::bail!("Server returned invalid JSON: {}", e);
            }

            conn.execute(
                "INSERT OR REPLACE INTO configs (key, value, updated_at) VALUES (?1, ?2, datetime('now'))",
                [config_key, &body],
            )?;

            println!("Successfully synced {} ({} bytes)", label, body.len());
            info!("Synced {}: {} bytes", label, body.len());
        }
        Err(e) => {
            error!("Failed to sync {}: {}", label, e);
            anyhow::bail!("Failed to connect to cloud: {}", e);
        }
    }

    Ok(())
}

fn open_db() -> Result<rusqlite::Connection, anyhow::Error> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");

    if !db_path.exists() {
        // Initialize with dh-db migrations to ensure configs table exists
        let _ = dh_db::DbManager::open(&db_path)?;
    }

    rusqlite::Connection::open(&db_path).map_err(Into::into)
}

fn get_config_value(
    conn: &rusqlite::Connection,
    key: &str,
) -> Result<Option<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT value FROM configs WHERE key = ?1")?;
    let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
    rows.next().transpose()
}

fn normalize_key(key: &str) -> String {
    key.to_lowercase().replace('-', "_")
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
