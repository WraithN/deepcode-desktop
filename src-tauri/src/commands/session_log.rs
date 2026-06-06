use crate::DbState;
use rusqlite::params;
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub fn session_log_load(
    state: State<'_, DbState>,
    conversation_id: String,
) -> Result<Vec<Value>, String> {
    let db_state = &*state;
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, timestamp, level, source, message, payload
             FROM session_logs
             WHERE conversation_id = ?1
             ORDER BY timestamp ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![&conversation_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "conversation_id": row.get::<_, String>(1)?,
                "timestamp": row.get::<_, String>(2)?,
                "level": row.get::<_, String>(3)?,
                "source": row.get::<_, String>(4)?,
                "message": row.get::<_, String>(5)?,
                "payload": row.get::<_, Option<String>>(6)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}
