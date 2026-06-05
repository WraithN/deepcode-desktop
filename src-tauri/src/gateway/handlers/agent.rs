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
    _session_manager: Arc<SessionManager>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let conversation_id = req.params.get("conversationId").and_then(|v| v.as_str());
    let message = req.params.get("message").and_then(|v| v.as_str());
    let opencode_session_id = req.params.get("opencodeSessionId").and_then(|v| v.as_str());
    
    if instance_id.is_none() || conversation_id.is_none() || message.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, conversationId, message", None);
    }

    match opencode_service.run_message(message.unwrap(), opencode_session_id).await {
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
