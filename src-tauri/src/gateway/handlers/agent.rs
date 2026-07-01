use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse, INTERNAL_ERROR, INVALID_PARAMS, INSTANCE_NOT_FOUND};
use crate::gateway::session_manager::SessionManager;
use crate::models::agent::CreateInstanceRequest;
use crate::service::agent_service::AgentService;
use serde_json::json;
use std::sync::Arc;

pub async fn handle_agent_request(
    service: Arc<AgentService>,
    _session_manager: Arc<SessionManager>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "agent.createInstance" => handle_create_instance(service, req).await,
        "agent.sendMessage" => handle_send_message(service, req).await,
        "agent.run" => handle_run(service, req).await,
        "agent.stopInstance" => handle_stop_instance(service, req).await,
        "agent.listInstances" => handle_list_instances(service, req).await,
        "agent.getInstance" => handle_get_instance(service, req).await,
        "agent.setMode" => handle_set_mode(service, req).await,
        "agent.respond" => handle_respond(service, req).await,
        _ => JsonRpcResponse::error(
            req.id,
            crate::gateway::codec::METHOD_NOT_FOUND,
            &format!("Method '{}' not found", req.method),
            None,
        ),
    }
}

async fn handle_create_instance(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let agent_key = req.params.get("agentKey").and_then(|v| v.as_str());
    let name = req.params.get("name").and_then(|v| v.as_str());
    let work_directory = req.params.get("workDirectory").and_then(|v| v.as_str());
    let force = req.params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    if agent_key.is_none() || name.is_none() || work_directory.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: agentKey, name, workDirectory", None);
    }

    let create_req = CreateInstanceRequest {
        agent_key: agent_key.unwrap().to_string(),
        name: name.unwrap().to_string(),
        work_directory: work_directory.unwrap().to_string(),
        force,
    };

    match service.create_instance(create_req).await {
        Ok(info) => JsonRpcResponse::success(req.id, json!({
            "instanceId": info.id,
            "status": info.status,
            "agentKey": info.agent_key,
            "name": info.name,
            "workDirectory": info.work_directory,
        })),
        Err(e) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &e.to_string(), None),
    }
}

async fn handle_send_message(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let conversation_id = req.params.get("conversationId").and_then(|v| v.as_str());
    let message = req.params.get("message").and_then(|v| v.as_str());

    if instance_id.is_none() || conversation_id.is_none() || message.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, conversationId, message", None);
    }

    match service.send_message(instance_id.unwrap(), conversation_id.unwrap(), message.unwrap()).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({"status": "started", "message": "Message processing started"})),
        Err(e) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &e.to_string(), None),
    }
}

async fn handle_respond(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let session_id = req.params.get("sessionId").and_then(|v| v.as_str());
    let interaction_type = req.params.get("interactionType").and_then(|v| v.as_str());
    let response = req.params.get("response").cloned();

    if instance_id.is_none() || session_id.is_none() || interaction_type.is_none() || response.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params", None);
    }

    let resp = response.unwrap();
    let message = match interaction_type.unwrap() {
        "question" => {
            if let Some(answers) = resp.get("answers").and_then(|v| v.as_array()) {
                answers.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n")
            } else {
                return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Invalid response format for question", None);
            }
        }
        "permission" => resp.get("answer").and_then(|v| v.as_str()).unwrap_or("deny").to_string(),
        "todowrite" => resp.get("todos").map(|v| v.to_string()).unwrap_or_default(),
        _ => return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Unknown interaction type", None),
    };

    match service.respond_to_instance(instance_id.unwrap(), session_id.unwrap(), &message).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({"status": "sent"})),
        Err(e) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &e.to_string(), None),
    }
}

async fn handle_run(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(req.id, json!({"status": "started"}))
}

async fn handle_stop_instance(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());

    if instance_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: instanceId", None);
    }

    match service.stop_instance(instance_id.unwrap()).await {
        Ok(()) => JsonRpcResponse::success(req.id, json!({"status": "stopped"})),
        Err(e) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &e.to_string(), None),
    }
}

async fn handle_list_instances(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instances = service.list_instances().await;
    JsonRpcResponse::success(req.id, json!(instances))
}

async fn handle_get_instance(service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());

    if instance_id.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: instanceId", None);
    }

    match service.get_instance(instance_id.unwrap()).await {
        Some(info) => JsonRpcResponse::success(req.id, json!(info)),
        None => JsonRpcResponse::error(req.id, INSTANCE_NOT_FOUND, "Instance not found", None),
    }
}

async fn handle_set_mode(_service: Arc<AgentService>, req: JsonRpcRequest) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let mode = req.params.get("mode").and_then(|v| v.as_str());

    if instance_id.is_none() || mode.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params: instanceId, mode", None);
    }

    JsonRpcResponse::success(req.id, json!({"status": "mode_set"}))
}
