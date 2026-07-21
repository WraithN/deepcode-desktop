#![allow(dead_code)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

/// 更新指定 session 中某个 agent 实例的模型配置。
/// 该端点由 dh-backend 调用，用于把 workspace 级别的模型设置同步到 gatewayd。
#[derive(Deserialize)]
pub struct UpdateAgentConfigRequest {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_type: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

pub async fn update_agent_config_handler(
    State(state): State<crate::ApiState>,
    Path((session_id, agent_id)): Path<(String, String)>,
    Json(req): Json<UpdateAgentConfigRequest>,
) -> impl IntoResponse {
    let Some(service) = state.agent_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Agent runtime not available"})),
        )
            .into_response();
    };

    let Some(session) = state.session_manager.get_session(&session_id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "session not found"})),
        )
            .into_response();
    };

    if !session.instances().contains(&agent_id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found in session"})),
        )
            .into_response();
    }

    let update_req = agent_core::models::UpdateModelConfigRequest {
        instance_id: agent_id,
        model_type: req.model_type,
        model_id: req.model_id.or(req.model.clone()),
        model_name: req.model,
        url: req.base_url,
        api_key: req.api_key,
        show_thinking: None,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
    };

    match service.update_model_config(update_req).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
