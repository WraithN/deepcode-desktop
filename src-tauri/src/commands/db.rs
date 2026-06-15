use serde_json::Value;
use std::sync::Arc;
use tauri::State;

use crate::DbState;
use dh_db::desktop::AppRepository;

fn repository(state: &State<DbState>) -> Result<AppRepository, String> {
    let conn = Arc::clone(&state.0);
    Ok(AppRepository::new(conn))
}

#[tauri::command]
pub fn db_sign_in(state: State<DbState>, username: String, password: String) -> Result<Value, String> {
    let repo = repository(&state)?;
    let user = repo.sign_in(&username, &password)?;
    Ok(serde_json::to_value(user).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn db_sign_up(state: State<DbState>, username: String, password: String) -> Result<Value, String> {
    let repo = repository(&state)?;
    let user = repo.sign_up(&username, &password)?;
    Ok(serde_json::to_value(user).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn db_get_profile(state: State<DbState>, user_id: String) -> Result<Option<Value>, String> {
    let repo = repository(&state)?;
    repo.get_profile(&user_id)
}

#[tauri::command]
pub fn db_load_conversations(
    state: State<DbState>,
    user_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let repo = repository(&state)?;
    repo.load_conversations(&user_id, limit)
}

#[tauri::command]
pub fn db_create_conversation(state: State<DbState>, data: Value) -> Result<Value, String> {
    let repo = repository(&state)?;
    repo.create_conversation(&data)
}

#[tauri::command]
pub fn db_update_conversation(state: State<DbState>, id: String, data: Value) -> Result<(), String> {
    let repo = repository(&state)?;
    repo.update_conversation(&id, &data)
}

#[tauri::command]
pub fn db_delete_conversation(state: State<DbState>, id: String) -> Result<(), String> {
    let repo = repository(&state)?;
    repo.delete_conversation(&id)
}

#[tauri::command]
pub fn db_load_messages(
    state: State<DbState>,
    conversation_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let repo = repository(&state)?;
    repo.load_messages(&conversation_id, limit)
}

#[tauri::command]
pub fn db_create_message(state: State<DbState>, data: Value) -> Result<Value, String> {
    let repo = repository(&state)?;
    repo.create_message(&data)
}

#[tauri::command]
pub fn db_load_tasks(
    state: State<DbState>,
    user_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let repo = repository(&state)?;
    repo.load_tasks(&user_id, limit)
}

#[tauri::command]
pub fn db_create_task(state: State<DbState>, data: Value) -> Result<Value, String> {
    let repo = repository(&state)?;
    repo.create_task(&data)
}

#[tauri::command]
pub fn db_load_modified_files(
    state: State<DbState>,
    user_id: String,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let repo = repository(&state)?;
    repo.load_modified_files(&user_id, limit)
}

#[tauri::command]
pub fn db_create_modified_file(state: State<DbState>, data: Value) -> Result<Value, String> {
    let repo = repository(&state)?;
    repo.create_modified_file(&data)
}
