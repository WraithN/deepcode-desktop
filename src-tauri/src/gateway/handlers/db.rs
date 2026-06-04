use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse, INVALID_PARAMS};
use crate::service::db_service::DbService;
use serde_json::json;
use std::sync::Arc;

pub async fn handle_db_request(
    service: Arc<DbService>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "db.signIn" => handle_sign_in(service, req).await,
        "db.signUp" => handle_sign_up(service, req).await,
        "db.getProfile" => handle_get_profile(service, req).await,
        "db.loadConversations" => handle_load_conversations(service, req).await,
        "db.createConversation" => handle_create_conversation(service, req).await,
        "db.updateConversation" => handle_update_conversation(service, req).await,
        "db.deleteConversation" => handle_delete_conversation(service, req).await,
        "db.loadMessages" => handle_load_messages(service, req).await,
        "db.createMessage" => handle_create_message(service, req).await,
        "db.loadTasks" => handle_load_tasks(service, req).await,
        "db.createTask" => handle_create_task(service, req).await,
        "db.loadModifiedFiles" => handle_load_modified_files(service, req).await,
        "db.createModifiedFile" => handle_create_modified_file(service, req).await,
        _ => JsonRpcResponse::error(
            req.id,
            crate::gateway::codec::METHOD_NOT_FOUND,
            &format!("Method '{}' not found", req.method),
            None,
        ),
    }
}

fn get_string_param(params: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn get_i64_param(params: &serde_json::Map<String, serde_json::Value>, key: &str, default: i64) -> i64 {
    params.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

async fn handle_sign_in(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let username = req.params.get("username").and_then(|v| v.as_str());
    let password = req.params.get("password").and_then(|v| v.as_str());
    if username.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing username", None);
    }
    match service.sign_in(username.unwrap().to_string(), password.unwrap_or("").to_string()) {
        Ok(user) => JsonRpcResponse::success(req.id, json!({
            "id": user.id,
            "email": user.email,
            "username": user.username,
            "created_at": user.created_at,
        })),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_sign_up(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let username = req.params.get("username").and_then(|v| v.as_str());
    let password = req.params.get("password").and_then(|v| v.as_str());
    if username.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing username", None);
    }
    match service.sign_up(username.unwrap().to_string(), password.unwrap_or("").to_string()) {
        Ok(user) => JsonRpcResponse::success(req.id, json!({
            "id": user.id,
            "email": user.email,
            "username": user.username,
            "created_at": user.created_at,
        })),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_get_profile(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let user_id = req.params.get("userId").and_then(|v| v.as_str());
    if user_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing userId", None);
    }
    match service.get_profile(user_id.unwrap().to_string()) {
        Ok(Some(profile)) => JsonRpcResponse::success(req.id, profile),
        Ok(None) => JsonRpcResponse::success(req.id, serde_json::json!(null)),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_load_conversations(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let user_id = req.params.get("userId").and_then(|v| v.as_str());
    let limit = req.params.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);
    if user_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing userId", None);
    }
    match service.load_conversations(user_id.unwrap().to_string(), limit) {
        Ok(data) => JsonRpcResponse::success(req.id, json!(data)),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_create_conversation(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    match service.create_conversation(req.params.into()) {
        Ok(data) => JsonRpcResponse::success(req.id, data),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_update_conversation(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let id = req.params.get("id").and_then(|v| v.as_str());
    let data = req.params.get("data").cloned().unwrap_or(json!({}));
    if id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing id", None);
    }
    match service.update_conversation(id.unwrap().to_string(), data) {
        Ok(()) => JsonRpcResponse::success(req.id, json!({})),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_delete_conversation(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let id = req.params.get("id").and_then(|v| v.as_str());
    if id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing id", None);
    }
    match service.delete_conversation(id.unwrap().to_string()) {
        Ok(()) => JsonRpcResponse::success(req.id, json!({})),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_load_messages(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let conversation_id = req.params.get("conversationId").and_then(|v| v.as_str());
    let limit = req.params.get("limit").and_then(|v| v.as_i64()).unwrap_or(100);
    if conversation_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing conversationId", None);
    }
    match service.load_messages(conversation_id.unwrap().to_string(), limit) {
        Ok(data) => JsonRpcResponse::success(req.id, json!(data)),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_create_message(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    match service.create_message(req.params.into()) {
        Ok(data) => JsonRpcResponse::success(req.id, data),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_load_tasks(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let user_id = req.params.get("userId").and_then(|v| v.as_str());
    let limit = req.params.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
    if user_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing userId", None);
    }
    match service.load_tasks(user_id.unwrap().to_string(), limit) {
        Ok(data) => JsonRpcResponse::success(req.id, json!(data)),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_create_task(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    match service.create_task(req.params.into()) {
        Ok(data) => JsonRpcResponse::success(req.id, data),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_load_modified_files(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let user_id = req.params.get("userId").and_then(|v| v.as_str());
    let limit = req.params.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
    if user_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing userId", None);
    }
    match service.load_modified_files(user_id.unwrap().to_string(), limit) {
        Ok(data) => JsonRpcResponse::success(req.id, json!(data)),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}

async fn handle_create_modified_file(service: Arc<DbService>, req: JsonRpcRequest) -> JsonRpcResponse {
    match service.create_modified_file(req.params.into()) {
        Ok(data) => JsonRpcResponse::success(req.id, data),
        Err(e) => JsonRpcResponse::error(req.id, 1, &e, None),
    }
}
