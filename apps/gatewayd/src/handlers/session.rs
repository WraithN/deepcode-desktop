#![allow(dead_code)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
    #[serde(default)]
    pub force: Option<bool>,
}

pub async fn create_session_handler(State(state): State<crate::ApiState>) -> impl IntoResponse {
    let session_id = state.session_manager.create_session();
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "sessionId": session_id })),
    )
}

pub async fn create_agent_handler(
    State(state): State<crate::ApiState>,
    Path(session_id): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    let Some(service) = state.agent_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Agent runtime not available" })),
        )
            .into_response();
    };

    match state
        .session_manager
        .create_agent(
            &session_id,
            &req.plugin_key,
            &req.name,
            &req.workspace,
            req.force.unwrap_or(false),
            service,
        )
        .await
    {
        Ok(info) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "instance_id": info.id,
                "plugin_key": info.plugin_key,
                "name": info.name,
                "status": info.status,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
