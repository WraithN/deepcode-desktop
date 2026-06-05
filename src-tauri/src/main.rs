// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
use agent_db::{AgentDbManager, agent_db_create_conversation, agent_db_load_conversations, agent_db_create_message, agent_db_load_messages, agent_db_delete_agent};
use base64::Engine;

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
fn window_control(window: tauri::Window, action: String) -> Result<(), String> {
    log::info!("[window_control] action={}", action);
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "toggle_maximize" => {
            if window.is_maximized().map_err(|e| e.to_string())? {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => window.close().map_err(|e| e.to_string()),
        _ => Err(format!("unknown window action: {}", action)),
    }
}

#[derive(serde::Serialize)]
struct WorkspaceFileNode {
    name: String,
    path: String,
    is_dir: bool,
    ignored: bool,
    children: Option<Vec<WorkspaceFileNode>>,
}

#[derive(serde::Serialize)]
struct WorkspaceFileContent {
    path: String,
    content: String,
    truncated: bool,
    is_image: bool,
}

fn ignored_workspace_entry(name: &str) -> bool {
    matches!(name, "node_modules" | ".git" | "target" | "dist" | ".dist" | ".next" | "build")
}

fn resolve_workspace_path(workspace: &str, relative_path: Option<&str>) -> Result<PathBuf, String> {
    let root = PathBuf::from(workspace).canonicalize().map_err(|e| format!("工作区不存在: {}", e))?;
    let target = match relative_path {
        Some(path) if !path.is_empty() => root.join(path),
        _ => root.clone(),
    };
    let canonical = target.canonicalize().map_err(|e| format!("路径不存在: {}", e))?;
    if !canonical.starts_with(&root) {
        return Err("禁止访问工作区外的路径".to_string());
    }
    Ok(canonical)
}

fn path_to_workspace_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn git_ignored(root: &Path, relative_path: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("check-ignore")
        .arg("-q")
        .arg(relative_path)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn is_image_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".gif") || lower.ends_with(".webp") || lower.ends_with(".svg") || lower.ends_with(".bmp")
}

fn image_mime(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/png"
    }
}

fn read_workspace_dir(root: &Path, dir: &Path, depth: usize, max_depth: usize) -> Result<Vec<WorkspaceFileNode>, String> {
    if depth > max_depth {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut nodes = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if ignored_workspace_entry(&name) {
            continue;
        }

        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let is_dir = metadata.is_dir();
        let relative_path = path_to_workspace_relative(root, &path);
        let ignored = git_ignored(root, &relative_path);
        let children = if is_dir && depth < max_depth {
            Some(read_workspace_dir(root, &path, depth + 1, max_depth)?)
        } else {
            None
        };

        nodes.push(WorkspaceFileNode {
            name,
            path: relative_path,
            is_dir,
            ignored,
            children,
        });
    }

    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(nodes)
}

#[tauri::command]
fn list_workspace_tree(workspace: String) -> Result<Vec<WorkspaceFileNode>, String> {
    let root = resolve_workspace_path(&workspace, None)?;
    read_workspace_dir(&root, &root, 0, 4)
}

#[tauri::command]
fn read_workspace_file(workspace: String, path: String) -> Result<WorkspaceFileContent, String> {
    let root = PathBuf::from(&workspace).canonicalize().map_err(|e| format!("工作区不存在: {}", e))?;
    let target = resolve_workspace_path(&workspace, Some(&path))?;
    let metadata = fs::metadata(&target).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("只能读取文件".to_string());
    }

    let relative_path = path_to_workspace_relative(&root, &target);
    let bytes = fs::read(&target).map_err(|e| e.to_string())?;

    if is_image_path(&relative_path) {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(WorkspaceFileContent {
            path: relative_path.clone(),
            content: format!("data:{};base64,{}", image_mime(&relative_path), encoded),
            truncated: false,
            is_image: true,
        });
    }

    let max_size = 512 * 1024;
    let truncated = bytes.len() > max_size;
    let readable = if truncated { &bytes[..max_size] } else { &bytes[..] };
    let content = String::from_utf8_lossy(readable).to_string();

    Ok(WorkspaceFileContent {
        path: relative_path,
        content,
        truncated,
        is_image: false,
    })
}

#[derive(serde::Serialize, Clone)]
struct GitStatusEntry {
    path: String,
    status: String,
}

#[derive(serde::Serialize)]
struct GitChangedFile {
    path: String,
    status: String,
    additions: u64,
    deletions: u64,
    diff: String,
}

fn git_status_entries(root: &Path) -> Result<Vec<GitStatusEntry>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .output()
        .map_err(|e| format!("读取 git 状态失败: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    let parts: Vec<&str> = raw.split('\0').filter(|part| !part.is_empty()).collect();
    let mut index = 0;

    while index < parts.len() {
        let item = parts[index];
        if item.len() < 4 {
            index += 1;
            continue;
        }

        let code = &item[..2];
        let path = item[3..].to_string();
        let status = if code == "??" {
            "U"
        } else if code.contains('A') {
            "A"
        } else if code.contains('D') {
            "D"
        } else if code.contains('R') {
            "R"
        } else if code.contains('M') {
            "M"
        } else {
            "M"
        };

        entries.push(GitStatusEntry { path, status: status.to_string() });
        index += if code.contains('R') || code.contains('C') { 2 } else { 1 };
    }

    Ok(entries)
}

#[tauri::command]
fn git_status_workspace(workspace: String) -> Result<Vec<GitStatusEntry>, String> {
    let root = resolve_workspace_path(&workspace, None)?;
    git_status_entries(&root)
}

fn git_numstat(root: &Path, path: &str) -> (u64, u64) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--numstat")
        .arg("--")
        .arg(path)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = raw.lines().next() {
                let mut parts = line.split('\t');
                let additions = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let deletions = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                return (additions, deletions);
            }
        }
    }

    (0, 0)
}

fn git_diff(root: &Path, path: &str, status: &str) -> String {
    if status == "U" {
        return String::new();
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--")
        .arg(path)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).to_string();
        }
    }

    String::new()
}

#[tauri::command]
fn git_changed_files(workspace: String) -> Result<Vec<GitChangedFile>, String> {
    let root = resolve_workspace_path(&workspace, None)?;
    let status_entries = git_status_entries(&root)?;

    Ok(status_entries
        .into_iter()
        .map(|entry| {
            let (additions, deletions) = git_numstat(&root, &entry.path);
            let diff = git_diff(&root, &entry.path, &entry.status);
            GitChangedFile {
                path: entry.path,
                status: entry.status,
                additions,
                deletions,
                diff,
            }
        })
        .collect())
}

#[tauri::command]
async fn agent_send_message_direct(message: String, session_id: Option<String>) -> Result<Value, String> {
    let mut cmd = tokio::process::Command::new("opencode");
    cmd.arg("run")
        .arg(&message)
        .arg("--format")
        .arg("json");

    if let Some(session_id) = session_id {
        if !session_id.is_empty() {
            cmd.arg("--session").arg(session_id);
        }
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().await.map_err(|e| format!("Failed to execute opencode run: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut session_id_result = String::new();
    let mut text_parts: Vec<String> = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(event) = serde_json::from_str::<Value>(line) {
            if session_id_result.is_empty() {
                if let Some(sid) = event.get("sessionID").or_else(|| event.get("sessionId")).or_else(|| event.get("session_id")).and_then(|v| v.as_str()) {
                    session_id_result = sid.to_string();
                }
            }

            if let Some(text) = event.get("content").or_else(|| event.get("text")).and_then(|v| v.as_str()) {
                text_parts.push(text.to_string());
            }

            if let Some(part) = event.get("part") {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(text.to_string());
                }
            }

            if let Some(parts) = event.get("parts").and_then(|v| v.as_array()) {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
            }
        }
    }

    if !output.status.success() {
        return Err(format!("opencode run failed: {}", stderr));
    }

    if text_parts.is_empty() {
        text_parts.push(if stderr.trim().is_empty() {
            "opencode 未返回内容".to_string()
        } else {
            stderr.trim().to_string()
        });
    }

    Ok(serde_json::json!({
        "sessionID": session_id_result,
        "parts": text_parts.iter().map(|text| serde_json::json!({ "type": "text", "text": text })).collect::<Vec<_>>(),
    }))
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

fn start_ws_server(
    mut ws_server: gateway::server::WebSocketServer,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<SocketAddr, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let result = ws_server.start(shutdown_rx).await.map_err(|e| e.to_string());
            let is_ok = result.is_ok();
            let _ = tx.send(result);
            if is_ok {
                std::future::pending::<()>().await;
            }
        });
    });
    rx.recv().map_err(|e| e.to_string())?
}

fn main() {
    env_logger::init();
    log::info!("[main.rs] Starting DeepHarness Desktop...");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            log::info!("[main.rs] Tauri setup callback started");
            
            let db_path = db_path(app);
            log::info!("[main.rs] Database path: {:?}", db_path);
            
            let conn = Connection::open(&db_path).expect("打开数据库失败");
            init_db(&conn).expect("初始化数据库失败");
            app.manage(DbState(Mutex::new(conn)));
            app.manage(AgentDbManager::new());
            log::info!("[main.rs] Database initialized");

            // 初始化 SessionLogger
            let app_handle = app.handle().clone();
            let logger_db_path = db_path.clone();
            let logger_conn = Connection::open(&logger_db_path).expect("打开日志数据库失败");
            let logger = std::sync::Arc::new(agent_core::logger::SessionLogger::new(app_handle, logger_conn));
            app.manage(logger.clone());
            log::info!("[main.rs] SessionLogger initialized");

            // 初始化 AgentService 并注册 opencode plugin
            let app_handle = app.handle().clone();
            let mut agent_service = Arc::new(service::agent_service::AgentService::new(logger.clone()));
            Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
                app_handle,
                logger.clone(),
            )));
            app.manage(agent_service.clone());
            log::info!("[main.rs] AgentService initialized");

            // 初始化服务和 SessionManager
            let db_conn = Connection::open(&db_path).expect("打开数据库失败");
            let db_service = Arc::new(service::db_service::DbService::new(Arc::new(Mutex::new(db_conn))));
            let opencode_service = Arc::new(service::opencode_service::OpencodeService::new().unwrap_or_else(|e| {
                log::warn!("[main.rs] Failed to initialize OpencodeService: {}, using fallback", e);
                service::opencode_service::OpencodeService::new_fallback()
            }));
            let session_manager = Arc::new(gateway::session_manager::SessionManager::new());
            log::info!("[main.rs] Services initialized");

            // 初始化 WebSocket server
            let router = Arc::new(gateway::router::GatewayRouter::new(
                agent_service,
                db_service,
                opencode_service,
                session_manager,
            ));
            let ws_server = gateway::server::WebSocketServer::new(router);
            let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
            let addr = start_ws_server(ws_server, shutdown_rx).unwrap();
            app.manage(commands::system::WebSocketState {
                addr: Mutex::new(Some(addr)),
            });
            log::info!("[main.rs] WebSocket server started on: {:?}", addr);

            log::info!("[main.rs] Tauri setup completed successfully");
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
            get_current_dir,
            window_control,
            list_workspace_tree,
            read_workspace_file,
            git_status_workspace,
            git_changed_files,
            agent_send_message_direct,
            commands::session_log::session_log_load,
            commands::system::get_websocket_url,
            commands::system::get_webview_html,
            commands::system::console_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
