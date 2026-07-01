#![allow(dead_code)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    #[serde(default)]
    pub plugin_key: Option<String>,
    /// agent_key 是 plugin_key 的别名，方便前端统一使用 agent_key 来指定 agent。
    #[serde(default)]
    pub agent_key: Option<String>,
    pub name: String,
    pub workspace: String,
    #[serde(default)]
    pub force: Option<bool>,
}

impl CreateAgentRequest {
    fn resolve_plugin_key(&self) -> Option<String> {
        self.plugin_key
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| self.agent_key.clone().filter(|s| !s.is_empty()))
    }
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

    let plugin_key = req.resolve_plugin_key().unwrap_or_default();
    if plugin_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "plugin_key or agent_key is required" })),
        )
            .into_response();
    }

    match state
        .session_manager
        .create_agent(
            &session_id,
            &plugin_key,
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
