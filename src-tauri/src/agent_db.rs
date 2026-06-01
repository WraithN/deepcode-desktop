use rusqlite::{params, Connection, Result as SqliteResult};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

pub struct AgentDbManager;

impl AgentDbManager {
    pub fn new() -> Self {
        Self
    }

    fn db_path(app_handle: &AppHandle, instance_id: &str) -> Result<PathBuf, String> {
        let mut path = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        path.push("agents");
        path.push(instance_id);
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        path.push("data.db");
        Ok(path)
    }

    fn get_connection(app_handle: &AppHandle, instance_id: &str) -> Result<Connection, String> {
        let path = Self::db_path(app_handle, instance_id)?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        Self::init_schema(&conn).map_err(|e| e.to_string())?;
        Ok(conn)
    }

    fn init_schema(conn: &Connection) -> SqliteResult<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model TEXT,
                created_at TEXT,
                updated_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                steps TEXT,
                is_complete INTEGER DEFAULT 0,
                token_in INTEGER,
                token_out INTEGER,
                duration_ms INTEGER,
                created_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS modified_files (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                file_path TEXT NOT NULL,
                change_type TEXT NOT NULL,
                diff TEXT,
                created_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_meta (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            [],
        )?;
        Ok(())
    }

    pub fn delete_agent_db(app_handle: &AppHandle, instance_id: &str) -> Result<(), String> {
        let mut path = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        path.push("agents");
        path.push(instance_id);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[tauri::command]
pub fn agent_db_create_conversation(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
    data: Value,
) -> Result<Value, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let title = data["title"].as_str().unwrap_or("");
    let model = data["model"].as_str().unwrap_or("");
    conn.execute(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, title, model, &now, &now],
    )
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "title": title,
        "model": model,
        "created_at": now,
        "updated_at": now,
    }))
}

#[tauri::command]
pub fn agent_db_load_conversations(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
) -> Result<Vec<Value>, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let mut stmt = conn
        .prepare("SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "model": row.get::<_, Option<String>>(2)?,
                "created_at": row.get::<_, Option<String>>(3)?,
                "updated_at": row.get::<_, Option<String>>(4)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}

#[tauri::command]
pub fn agent_db_create_message(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
    data: Value,
) -> Result<Value, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let conversation_id = data["conversation_id"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("");
    let content = data["content"].as_str().unwrap_or("");
    let steps = data["steps"].as_str();
    let is_complete = data["is_complete"].as_bool().unwrap_or(false) as i32;
    let token_in = data["token_in"].as_i64();
    let token_out = data["token_out"].as_i64();
    let duration_ms = data["duration_ms"].as_i64();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, steps, is_complete, token_in, token_out, duration_ms, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![&id, conversation_id, role, content, steps, is_complete, token_in, token_out, duration_ms, &now],
    )
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "conversation_id": conversation_id,
        "role": role,
        "content": content,
        "created_at": now,
    }))
}

#[tauri::command]
pub fn agent_db_load_messages(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
    conversation_id: String,
) -> Result<Vec<Value>, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let mut stmt = conn
        .prepare("SELECT id, conversation_id, role, content, steps, is_complete, token_in, token_out, duration_ms, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![&conversation_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "conversation_id": row.get::<_, String>(1)?,
                "role": row.get::<_, String>(2)?,
                "content": row.get::<_, String>(3)?,
                "steps": row.get::<_, Option<String>>(4)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "is_complete": row.get::<_, i32>(5)? == 1,
                "token_in": row.get::<_, Option<i64>>(6)?,
                "token_out": row.get::<_, Option<i64>>(7)?,
                "duration_ms": row.get::<_, Option<i64>>(8)?,
                "created_at": row.get::<_, String>(9)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}

#[tauri::command]
pub fn agent_db_delete_agent(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
) -> Result<(), String> {
    AgentDbManager::delete_agent_db(&app_handle, &instance_id)
}
