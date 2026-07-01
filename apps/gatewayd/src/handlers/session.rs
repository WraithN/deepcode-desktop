#![allow(dead_code)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub agent_key: String,
    pub name: String,
    pub work_directory: String,
    #[serde(default)]
    pub force: Option<bool>,
}

/// `POST /sessions` 可选请求体，用于指定空闲超时时间。
#[derive(Deserialize, Default)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub expired_time: Option<u64>,
}

pub async fn create_session_handler(
    State(state): State<crate::ApiState>,
    body: Option<Json<CreateSessionRequest>>,
) -> impl IntoResponse {
    let expired = body.and_then(|Json(b)| b.expired_time);
    let session_id = state.session_manager.create_session(expired);
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

    if req.agent_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "agent_key is required" })),
        )
            .into_response();
    }

    match state
        .session_manager
        .create_agent(
            &session_id,
            &req.agent_key,
            &req.name,
            &req.work_directory,
            req.force.unwrap_or(false),
            service,
        )
        .await
    {
        Ok(info) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "instance_id": info.id,
                "agent_key": info.agent_key,
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
