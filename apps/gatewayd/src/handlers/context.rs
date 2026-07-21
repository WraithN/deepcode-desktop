use crate::ApiState;
use axum::extract::State;
use axum::response::Json;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value;
use tracing::info;

#[derive(Deserialize)]
pub struct ContextPayload {
    pub agent_type: String,
    pub session_id: String,
    pub work_directory: Option<String>,
    pub model: Option<String>,
}

pub async fn set_context(
    State(state): State<ApiState>,
    Json(payload): Json<ContextPayload>,
) -> Json<Value> {
    let mut guard = state.agent_type.lock().unwrap();
    *guard = Some(payload.agent_type.clone());
    let model = payload.model.as_deref().unwrap_or("unknown");
    let _ = upsert_session(
        &state.db_path,
        &payload.session_id,
        &payload.agent_type,
        model,
        payload.work_directory.as_deref(),
    );
    info!(
        "Context updated: agent_type = {}, session = {}",
        payload.agent_type, payload.session_id
    );
    Json(
        serde_json::json!({"status": "ok", "agent_type": payload.agent_type, "session_id": payload.session_id}),
    )
}

pub(crate) fn open_db(path: &std::path::Path) -> anyhow::Result<Connection> {
    Connection::open(path).map_err(Into::into)
}

pub(crate) fn upsert_session(
    db_path: &std::path::Path,
    session_id: &str,
    agent_type: &str,
    model: &str,
    work_directory: Option<&str>,
) -> anyhow::Result<()> {
    let conn = open_db(db_path)?;
    let work_directory = work_directory.unwrap_or("");
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, agent_type, model, workspace, started_at, last_active_at, status)          VALUES (?1, ?2, ?3, ?4, ?5, ?5, 'active')          ON CONFLICT(id) DO UPDATE SET          agent_type = excluded.agent_type,          model = excluded.model,          workspace = excluded.workspace,          last_active_at = excluded.last_active_at",
        params![session_id, agent_type, model, work_directory, now],
    )?;
    Ok(())
}

pub(crate) fn touch_session(db_path: &std::path::Path, session_id: &str, model: &str) -> anyhow::Result<()> {
    let conn = open_db(db_path)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET last_active_at = ?1, model = ?2 WHERE id = ?3",
        params![now, model, session_id],
    )?;
    Ok(())
}
