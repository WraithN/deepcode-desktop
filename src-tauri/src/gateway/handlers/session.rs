use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse};
use serde_json::json;

pub async fn handle_session_request(req: JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "session.logLoad" => {
            JsonRpcResponse::success(req.id, json!([]))
        }
        _ => JsonRpcResponse::error(
            req.id,
            crate::gateway::codec::METHOD_NOT_FOUND,
            &format!("Method '{}' not found", req.method),
            None,
        ),
    }
}
