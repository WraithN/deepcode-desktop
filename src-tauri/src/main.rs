// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
use sidecar_manager::{SidecarManager, check_opencode_installed, get_sidecar_status, start_sidecar, stop_sidecar};
use agent_db::{AgentDbManager, agent_db_create_conversation, agent_db_load_conversations, agent_db_create_message, agent_db_load_messages, agent_db_delete_agent};

mod sidecar_manager;
mod agent_db;
mod commands;
mod models;
mod service;
mod gateway;

use ai_coding_desktop::DbState;

fn db_path(app_handle: &tauri::App) -> PathBuf {
    let mut path = app_handle.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    fs::create_dir_all(&path).ok();
    path.push("app.db");
    path
}

fn init_db(conn: &Connection) -> SqliteResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS profiles (
            id TEXT PRIMARY KEY,
            username TEXT UNIQUE,
            email TEXT,
            phone TEXT,
            role TEXT DEFAULT 'user',
            created_at TEXT
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            title TEXT NOT NULL,
            agent TEXT NOT NULL,
            model TEXT NOT NULL,
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
            user_id TEXT NOT NULL,
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
            user_id TEXT NOT NULL,
            conversation_id TEXT,
            file_path TEXT NOT NULL,
            change_type TEXT NOT NULL,
            diff TEXT,
            created_at TEXT
        )",
        [],
    )?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct AuthUser {
    id: String,
    email: String,
    username: String,
    created_at: String,
}

#[tauri::command]
fn get_current_dir() -> Result<String, String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn db_sign_in(
    state: State<DbState>,
    username: String,
    _password: String,
) -> Result<AuthUser, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, username, email, created_at FROM profiles WHERE username = ?1")
        .map_err(|e| e.to_string())?;
    let user = stmt
        .query_row([&username], |row| {
            Ok(AuthUser {
                id: row.get(0)?,
                username: row.get(1)?,
                email: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|_| "用户不存在".to_string())?;
    Ok(user)
}

#[tauri::command]
fn db_sign_up(
    state: State<DbState>,
    username: String,
    _password: String,
) -> Result<AuthUser, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let email = format!("{}@local.dev", username);
    let created_at = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO profiles (id, username, email, phone, role, created_at) VALUES (?1, ?2, ?3, NULL, 'user', ?4)",
        params![&id, &username, &email, &created_at],
    ).map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "用户名已存在".to_string()
        } else {
            e.to_string()
        }
    })?;

    Ok(AuthUser {
        id,
        email,
        username,
        created_at,
    })
}

#[tauri::command]
fn db_get_profile(state: State<DbState>, user_id: String) -> Result<Option<Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, username, email, phone, role, created_at FROM profiles WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    let row = stmt
        .query_row([&user_id], |row| {
            let val = serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "username": row.get::<_, Option<String>>(1)?,
                "email": row.get::<_, Option<String>>(2)?,
                "phone": row.get::<_, Option<String>>(3)?,
                "role": row.get::<_, String>(4)?,
                "created_at": row.get::<_, String>(5)?,
            });
            Ok(val)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(row)
}

#[tauri::command]
fn db_load_conversations(
    state: State<DbState>,
    user_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, user_id, title, agent, model, created_at, updated_at FROM conversations WHERE user_id = ?1 ORDER BY updated_at DESC LIMIT ?2")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![&user_id, limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "user_id": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "agent": row.get::<_, String>(3)?,
                "model": row.get::<_, String>(4)?,
                "created_at": row.get::<_, String>(5)?,
                "updated_at": row.get::<_, String>(6)?,
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
fn db_create_conversation(
    state: State<DbState>,
    data: Value,
) -> Result<Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let user_id = data["user_id"].as_str().unwrap_or("");
    let title = data["title"].as_str().unwrap_or("");
    let agent = data["agent"].as_str().unwrap_or("");
    let model = data["model"].as_str().unwrap_or("");
    conn.execute(
        "INSERT INTO conversations (id, user_id, title, agent, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![&id, user_id, title, agent, model, &now, &now],
    ).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "user_id": user_id,
        "title": title,
        "agent": agent,
        "model": model,
        "created_at": now,
        "updated_at": now,
    }))
}

#[tauri::command]
fn db_update_conversation(
    state: State<DbState>,
    id: String,
    data: Value,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(title) = data["title"].as_str() {
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, chrono::Utc::now().to_rfc3339(), &id],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn db_delete_conversation(state: State<DbState>, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM conversations WHERE id = ?1", [&id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn db_load_messages(
    state: State<DbState>,
    conversation_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, conversation_id, role, content, steps, is_complete, token_in, token_out, duration_ms, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC LIMIT ?2")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![&conversation_id, limit], |row| {
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
fn db_create_message(state: State<DbState>, data: Value) -> Result<Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let conversation_id = data["conversation_id"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("");
    let content = data["content"].as_str().unwrap_or("");
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, conversation_id, role, content, &now],
    ).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "conversation_id": conversation_id,
        "role": role,
        "content": content,
        "created_at": now,
    }))
}

#[tauri::command]
fn db_load_tasks(
    state: State<DbState>,
    user_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, user_id, conversation_id, title, status, created_at FROM tasks WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![&user_id, limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "user_id": row.get::<_, String>(1)?,
                "conversation_id": row.get::<_, Option<String>>(2)?,
                "title": row.get::<_, String>(3)?,
                "status": row.get::<_, String>(4)?,
                "created_at": row.get::<_, String>(5)?,
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
fn db_create_task(state: State<DbState>, data: Value) -> Result<Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let user_id = data["user_id"].as_str().unwrap_or("");
    let conversation_id = data["conversation_id"].as_str();
    let title = data["title"].as_str().unwrap_or("");
    let status = data["status"].as_str().unwrap_or("");
    conn.execute(
        "INSERT INTO tasks (id, user_id, conversation_id, title, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![&id, user_id, conversation_id, title, status, &now],
    ).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "user_id": user_id,
        "conversation_id": conversation_id,
        "title": title,
        "status": status,
        "created_at": now,
    }))
}

#[tauri::command]
fn db_load_modified_files(
    state: State<DbState>,
    user_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, user_id, conversation_id, file_path, change_type, diff, created_at FROM modified_files WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![&user_id, limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "user_id": row.get::<_, String>(1)?,
                "conversation_id": row.get::<_, Option<String>>(2)?,
                "file_path": row.get::<_, String>(3)?,
                "change_type": row.get::<_, String>(4)?,
                "diff": row.get::<_, Option<String>>(5)?,
                "created_at": row.get::<_, String>(6)?,
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
fn db_create_modified_file(state: State<DbState>, data: Value) -> Result<Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let user_id = data["user_id"].as_str().unwrap_or("");
    let conversation_id = data["conversation_id"].as_str();
    let file_path = data["file_path"].as_str().unwrap_or("");
    let change_type = data["change_type"].as_str().unwrap_or("");
    let diff = data["diff"].as_str();
    conn.execute(
        "INSERT INTO modified_files (id, user_id, conversation_id, file_path, change_type, diff, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![&id, user_id, conversation_id, file_path, change_type, diff, &now],
    ).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "user_id": user_id,
        "conversation_id": conversation_id,
        "file_path": file_path,
        "change_type": change_type,
        "diff": diff,
        "created_at": now,
    }))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let db_path = db_path(app);
            let conn = Connection::open(&db_path).expect("打开数据库失败");
            init_db(&conn).expect("初始化数据库失败");
            app.manage(DbState(Mutex::new(conn)));
            app.manage(AgentDbManager::new());
            app.manage(SidecarManager::new());

            // 初始化 SessionLogger
            let app_handle = app.handle().clone();
            let logger_db_path = db_path.clone();
            let logger_conn = Connection::open(&logger_db_path).expect("打开日志数据库失败");
            let logger = std::sync::Arc::new(agent_core::logger::SessionLogger::new(app_handle, logger_conn));
            app.manage(logger.clone());

            // 初始化 AgentService 并注册 opencode plugin
            let app_handle = app.handle().clone();
            let mut agent_service = service::agent_service::AgentService::new(logger.clone());
            agent_service.register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
                app_handle,
                logger.clone(),
            )));
            app.manage(agent_service);

            // 启动 Sidecar 健康检查后台线程（每 5 秒）
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    if let Some(manager) = app_handle.try_state::<SidecarManager>() {
                        manager.health_check();
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_sign_in,
            db_sign_up,
            db_get_profile,
            db_load_conversations,
            db_create_conversation,
            db_update_conversation,
            db_delete_conversation,
            db_load_messages,
            db_create_message,
            db_load_tasks,
            db_create_task,
            db_load_modified_files,
            db_create_modified_file,
            agent_db_create_conversation,
            agent_db_load_conversations,
            agent_db_create_message,
            agent_db_load_messages,
            agent_db_delete_agent,
            start_sidecar,
            stop_sidecar,
            get_sidecar_status,
            check_opencode_installed,
            get_current_dir,
            commands::agent::agent_list_plugins,
            commands::agent::agent_create_instance,
            commands::agent::agent_send_message,
            commands::agent::agent_stop_instance,
            commands::agent::agent_get_instance,
            commands::agent::agent_list_instances,
            commands::agent::agent_test_emit,
            commands::agent::agent_test_emit_agent_event,
            commands::session_log::session_log_load,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
