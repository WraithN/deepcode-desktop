use crate::ApiState;
use axum::extract::State;
use axum::response::Json;
use dh_db::DbManager;
use serde_json::Value;

pub async fn health_check() -> Json<Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "gatewayd",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub async fn reporter_status_handler(State(state): State<ApiState>) -> Json<Value> {
    let cursor = match DbManager::open(&state.db_path) {
        Ok(db) => db.get_reporter_cursor().unwrap_or(0),
        Err(_) => 0,
    };
    let (pending, dead) = match DbManager::open(&state.db_path) {
        Ok(db) => db.get_queue_stats().unwrap_or((0, 0)),
        Err(_) => (0, 0),
    };

    Json(serde_json::json!({
        "enabled": std::env::var("DH_REPORTER_ENABLED").is_ok(),
        "endpoint": std::env::var("DH_REPORTER_ENDPOINT").ok(),
        "last_sync_rowid": cursor,
        "queue_pending": pending,
        "queue_dead": dead,
    }))
}

pub async fn rotate_api_key_handler(State(state): State<ApiState>) -> Json<Value> {
    let new_key = match crate::auth::ApiKeyStore::rotate(&state.db_path) {
        Ok(k) => k,
        Err(e) => {
            return Json(serde_json::json!({
                "status": "error",
                "error": e.to_string(),
            }));
        }
    };
    Json(serde_json::json!({
        "status": "ok",
        "message": "API key rotated. Update clients with the new key.",
        "key": new_key,
    }))
}
