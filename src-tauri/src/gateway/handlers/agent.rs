use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse, INVALID_PARAMS, PLUGIN_NOT_FOUND, INSTANCE_NOT_FOUND, INSTANCE_LIMIT_EXCEEDED, PROCESS_SPAWN_FAILED, MCP_INIT_FAILED};
use crate::service::agent_service::AgentService;
use serde_json::json;
use std::sync::Arc;

pub async fn handle_agent_request(
    service: Arc<AgentService>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "agent.createInstance" => handle_create_instance(service, req).await,
        "agent.sendMessage" => handle_send_message(service, req).await,
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

async fn handle_create_instance(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
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

async fn handle_send_message(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let conversation_id = req.params.get("conversationId").and_then(|v| v.as_str());
    let message = req.params.get("message").and_then(|v| v.as_str());
    
    if instance_id.is_none() || message.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, message", None);
    }
    
    JsonRpcResponse::success(req.id, json!({"status": "dispatched"}))
}

async fn handle_stop_instance(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    
    if instance_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: instanceId", None);
    }
    
    JsonRpcResponse::success(req.id, json!({"status": "stopped"}))
}

async fn handle_list_instances(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(req.id, json!([]))
}

async fn handle_get_instance(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    
    if instance_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: instanceId", None);
    }
    
    JsonRpcResponse::error(req.id, INSTANCE_NOT_FOUND, "Instance not found", None)
}

async fn handle_set_mode(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let mode = req.params.get("mode").and_then(|v| v.as_str());
    
    if instance_id.is_none() || mode.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, mode", None);
    }
    
    JsonRpcResponse::success(req.id, json!({"status": "mode_set"}))
}
