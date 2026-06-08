use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse};
use crate::service::db_service::DbService;
use serde_json::json;
use std::sync::Arc;

pub async fn handle_session_request(req: JsonRpcRequest, db_service: Arc<DbService>) -> JsonRpcResponse {
    match req.method.as_str() {
        "session.logLoad" => {
            let conversation_id = req.params
                .get("conversationId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if conversation_id.is_empty() {
                return JsonRpcResponse::error(req.id, -32602, "Missing conversationId", None);
            }
            match db_service.load_session_logs(conversation_id) {
                Ok(logs) => {
                    let camel_logs: Vec<serde_json::Value> = logs.into_iter().map(|entry| {
                        let mut e = entry.as_object().cloned().unwrap_or_default();
                        if let Some(v) = e.remove("conversation_id") {
                            e.insert("conversationId".to_string(), v);
                        }
                        if let Some(v) = e.remove("instance_id") {
                            e.insert("instanceId".to_string(), v);
                        }
                        if let Some(v) = e.get("id") {
                            if v.is_number() {
                                e.insert("id".to_string(), serde_json::Value::String(v.to_string()));
                            }
                        }
                        serde_json::Value::Object(e)
                    }).collect();
                    JsonRpcResponse::success(req.id, json!(camel_logs))
                }
                Err(e) => JsonRpcResponse::error(req.id, -32603, &e, None),
            }
        }
        _ => JsonRpcResponse::error(
            req.id,
            crate::gateway::codec::METHOD_NOT_FOUND,
            &format!("Method '{}' not found", req.method),
            None,
        ),
    }
}
