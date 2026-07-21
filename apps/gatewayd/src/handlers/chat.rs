use crate::ApiState;
use crate::audit::log_response_audit;
use crate::gateway::{anthropic_to_unified, openai_to_unified, resolve_provider, unified_to_anthropic_json, unified_to_openai_json};
use crate::handlers::context::touch_session;
use crate::server::MAX_RESPONSE_BODY_BYTES;
use axum::body::{Body, Bytes};
use axum::response::Response;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use dh_core::{AuditLogEntry, Direction};
use serde_json::Value;
use tracing::{error, info};

pub async fn openai_chat_completions(State(state): State<ApiState>, body: Bytes) -> Response {
    info!("Received OpenAI chat completions request");

    let body_str = String::from_utf8_lossy(&body);
    let body_json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    let mut unified = openai_to_unified(body_json.clone());
    let original_size = serde_json::to_string(&body_json).unwrap_or_default().len();
    state.rtk.optimize(&mut unified);
    let optimized_json = unified_to_openai_json(&unified);
    let optimized_body = serde_json::to_string(&optimized_json).unwrap_or_default();
    let optimized_size = optimized_body.len();

    info!(
        "RTK optimized OpenAI request: {} -> {} bytes ({}% reduction)",
        original_size,
        optimized_size,
        percentage_reduction(original_size, optimized_size)
    );

    let is_streaming = unified.stream;
    let session_id = unified.session_id.clone();
    let mut entry = AuditLogEntry::new(
        session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        resolve_provider(&unified.model).to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = optimized_size;
    entry.payload = Some(optimized_body.clone());
    entry.agent_type = state.agent_type.lock().unwrap().clone();
    let _ = touch_session(&state.db_path, &session_id, &unified.model);
    state.audit.log(entry);

    let provider = resolve_provider(&unified.model);
    match state
        .router
        .forward_openai(provider, optimized_body.clone())
        .await
    {
        Ok(response) if is_streaming => {
            info!("Successfully forwarded streaming request to {}", provider);
            response
        }
        Ok(response) => {
            info!("Successfully forwarded request to {}", provider);
            let (parts, body) = response.into_parts();
            match axum::body::to_bytes(body, MAX_RESPONSE_BODY_BYTES).await {
                Ok(bytes) => {
                    log_response_audit(
                        state.audit.as_ref(),
                        session_id.clone(),
                        unified.id.clone(),
                        provider.to_string(),
                        unified.model.clone(),
                        &bytes,
                        &optimized_body,
                    );
                    let body = Body::from(bytes);
                    axum::response::Response::from_parts(parts, body)
                }
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    (
                        StatusCode::BAD_GATEWAY,
                        "Gateway error: failed to read response",
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            error!("Failed to forward request to {}: {}", provider, e);
            (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
        }
    }
}

/// Returns the percentage reduction from `original` to `optimized`, or 0 if
/// there is no original size to compare against.
fn percentage_reduction(original: usize, optimized: usize) -> i32 {
    if original == 0 {
        return 0;
    }
    let saved = original.saturating_sub(optimized);
    saved
        .saturating_mul(100)
        .checked_div(original)
        .unwrap_or(0) as i32
}

pub async fn anthropic_messages(State(state): State<ApiState>, body: Bytes) -> Response {
    info!("Received Anthropic messages request");

    let body_str = String::from_utf8_lossy(&body);
    let body_json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    let mut unified = anthropic_to_unified(body_json.clone());
    let original_size = serde_json::to_string(&body_json).unwrap_or_default().len();
    state.rtk.optimize(&mut unified);
    let optimized_json = unified_to_anthropic_json(&unified);
    let optimized_body = serde_json::to_string(&optimized_json).unwrap_or_default();
    let optimized_size = optimized_body.len();

    info!(
        "RTK optimized Anthropic request: {} -> {} bytes ({}% reduction)",
        original_size,
        optimized_size,
        percentage_reduction(original_size, optimized_size)
    );

    let is_streaming = unified.stream;
    let session_id = unified.session_id.clone();
    let mut entry = AuditLogEntry::new(
        session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        resolve_provider(&unified.model).to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = optimized_size;
    entry.payload = Some(optimized_body.clone());
    entry.agent_type = state.agent_type.lock().unwrap().clone();
    let _ = touch_session(&state.db_path, &session_id, &unified.model);
    state.audit.log(entry);

    let provider = resolve_provider(&unified.model);
    match state.router.forward_anthropic(optimized_body.clone()).await {
        Ok(response) if is_streaming => {
            info!("Successfully forwarded streaming request to {}", provider);
            response
        }
        Ok(response) => {
            info!("Successfully forwarded request to {}", provider);
            let (parts, body) = response.into_parts();
            match axum::body::to_bytes(body, MAX_RESPONSE_BODY_BYTES).await {
                Ok(bytes) => {
                    log_response_audit(
                        state.audit.as_ref(),
                        session_id.clone(),
                        unified.id.clone(),
                        provider.to_string(),
                        unified.model.clone(),
                        &bytes,
                        &optimized_body,
                    );
                    let body = Body::from(bytes);
                    axum::response::Response::from_parts(parts, body)
                }
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    (
                        StatusCode::BAD_GATEWAY,
                        "Gateway error: failed to read response",
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            error!("Failed to forward request to {}: {}", provider, e);
            (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
        }
    }
}
