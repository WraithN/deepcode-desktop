use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse, INTERNAL_ERROR, INVALID_PARAMS, INSTANCE_NOT_FOUND};
use crate::gateway::session_manager::SessionManager;
use crate::service::agent_service::AgentService;
use crate::service::opencode_service::OpencodeService;
use serde_json::json;
use std::sync::Arc;


pub async fn handle_agent_request(
    service: Arc<AgentService>,
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "agent.createInstance" => handle_create_instance(service, req).await,
        "agent.sendMessage" => handle_send_message(opencode_service, session_manager, req).await,
        "agent.run" => handle_run(service, req).await,
        "agent.stopInstance" => handle_stop_instance(service, req).await,
        "agent.listInstances" => handle_list_instances(service, req).await,
        "agent.getInstance" => handle_get_instance(service, req).await,
        "agent.setMode" => handle_set_mode(service, req).await,
        "agent.respond" => handle_respond(opencode_service, req).await,
        _ => JsonRpcResponse::error(
            req.id,
            crate::gateway::codec::METHOD_NOT_FOUND,
            &format!("Method '{}' not found", req.method),
            None,
        ),
    }
}

async fn handle_create_instance(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let plugin_key = req.params.get("pluginKey").and_then(|v| v.as_str());
    let name = req.params.get("name").and_then(|v| v.as_str());
    let workspace = req.params.get("workspace").and_then(|v| v.as_str());
    
    if plugin_key.is_none() || name.is_none() || workspace.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: pluginKey, name, workspace", None);
    }
    
    // TODO: Delegate to AgentService (will be implemented in later tasks)
    JsonRpcResponse::success(req.id, json!({
        "instanceId": "placeholder",
        "status": "running",
        "pluginKey": plugin_key,
        "name": name,
        "workspace": workspace,
    }))
}

async fn handle_send_message(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let conversation_id = req.params.get("conversationId").and_then(|v| v.as_str());
    let message = req.params.get("message").and_then(|v| v.as_str());
    let opencode_session_id = req.params.get("opencodeSessionId").and_then(|v| v.as_str());

    if instance_id.is_none() || conversation_id.is_none() || message.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, conversationId, message", None);
    }

    let conversation_id_str = conversation_id.unwrap().to_string();
    let message_str = message.unwrap().to_string();
    let opencode_session_id_opt = opencode_session_id.map(|s| s.to_string());

    // 在后台启动流式处理，立即返回，避免阻塞 WebSocket
    let opencode_service_clone = opencode_service.clone();
    let session_manager_clone = session_manager.clone();
    tokio::spawn(async move {
        crate::gateway::handlers::streaming::stream_opencode_output(
            opencode_service_clone,
            session_manager_clone,
            conversation_id_str,
            message_str,
            opencode_session_id_opt,
        ).await;
    });

    JsonRpcResponse::success(req.id, json!({
        "status": "started",
        "message": "Message processing started"
    }))
}

async fn handle_respond(
    opencode_service: Arc<OpencodeService>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let session_id = req.params.get("sessionId").and_then(|v| v.as_str());
    let interaction_type = req.params.get("interactionType").and_then(|v| v.as_str());
    let response = req.params.get("response").cloned();

    if session_id.is_none() || interaction_type.is_none() || response.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params", None);
    }

    let sid = session_id.unwrap();
    let resp = response.unwrap();

    // Format response as message to send back to opencode
    let message = match interaction_type.unwrap() {
        "question" => {
            if let Some(answers) = resp.get("answers").and_then(|v| v.as_array()) {
                let texts: Vec<String> = answers.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                texts.join("\n")
            } else {
                return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Invalid response format for question", None);
            }
        }
        "permission" => {
            resp.get("answer").and_then(|v| v.as_str()).unwrap_or("deny").to_string()
        }
        "todowrite" => {
            resp.get("todos").map(|v| v.to_string()).unwrap_or_default()
        }
        _ => return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Unknown interaction type", None),
    };

    match opencode_service.send_message(sid, &message).await {
        Ok(result) => JsonRpcResponse::success(req.id, result),
        Err(error) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &error, None),
    }
}

async fn handle_run(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    // agent.run is handled by the streaming system
    // This is a placeholder - actual streaming is triggered via WebSocket notifications
    JsonRpcResponse::success(req.id, json!({"status": "started"}))
}

async fn handle_stop_instance(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    
    if instance_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: instanceId", None);
    }
    
    JsonRpcResponse::success(req.id, json!({"status": "stopped"}))
}

async fn handle_list_instances(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(req.id, json!([]))
}

async fn handle_get_instance(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    
    if instance_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: instanceId", None);
    }
    
    JsonRpcResponse::error(req.id, INSTANCE_NOT_FOUND, "Instance not found", None)
}

async fn handle_set_mode(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let mode = req.params.get("mode").and_then(|v| v.as_str());
    
    if instance_id.is_none() || mode.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, mode", None);
    }
    
    JsonRpcResponse::success(req.id, json!({"status": "mode_set"}))
}
