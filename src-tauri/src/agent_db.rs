use dh_db::desktop::AgentRepository;
use rusqlite::Connection;
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
        AgentRepository::init_schema(&conn).map_err(|e| e.to_string())?;
        Ok(conn)
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
    let repo = AgentRepository::new(&conn);
    repo.create_conversation(&data)
}

#[tauri::command]
pub fn agent_db_load_conversations(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
) -> Result<Vec<Value>, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let repo = AgentRepository::new(&conn);
    repo.load_conversations()
}

#[tauri::command]
pub fn agent_db_create_message(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
    data: Value,
) -> Result<Value, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let repo = AgentRepository::new(&conn);
    repo.create_message(&data)
}

#[tauri::command]
pub fn agent_db_load_messages(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
    conversation_id: String,
) -> Result<Vec<Value>, String> {
    let conn = AgentDbManager::get_connection(&app_handle, &instance_id)?;
    let repo = AgentRepository::new(&conn);
    repo.load_messages(&conversation_id)
}

#[tauri::command]
pub fn agent_db_delete_agent(
    app_handle: AppHandle,
    _state: State<AgentDbManager>,
    instance_id: String,
) -> Result<(), String> {
    AgentDbManager::delete_agent_db(&app_handle, &instance_id)
}
