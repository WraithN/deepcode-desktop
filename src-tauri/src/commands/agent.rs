use crate::models::agent::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use crate::service::agent_service::AgentService;
use agent_core::event::AgentEvent;
use agent_core::logger::SessionLogger;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn agent_list_plugins(service: State<'_, AgentService>) -> Result<Vec<PluginInfo>, String> {
    Ok(service.list_plugins())
}

#[tauri::command]
pub async fn agent_create_instance(
    service: State<'_, AgentService>,
    plugin_key: String,
    name: String,
    workspace: String,
) -> Result<InstanceInfo, String> {
    let req = CreateInstanceRequest {
        plugin_key,
        name,
        workspace,
    };
    service.create_instance(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn agent_send_message(
    service: State<'_, AgentService>,
    logger: State<'_, Arc<SessionLogger>>,
    instance_id: String,
    message: String,
    conversation_id: String,
) -> Result<(), String> {
    logger.log(
        &conversation_id,
        agent_core::logger::LogLevel::Info,
        "agent-service",
        "send_message called",
        Some(serde_json::json!({ "instance_id": &instance_id, "message": &message })),
    );

    match service.send_message(&instance_id, &message).await {
        Ok(_) => {
            logger.log(
                &conversation_id,
                agent_core::logger::LogLevel::Info,
                "agent-service",
                "message dispatched",
                None,
            );
            Ok(())
        }
        Err(e) => {
            logger.log(
                &conversation_id,
                agent_core::logger::LogLevel::Error,
                "agent-service",
                "send_message failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            );
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn agent_stop_instance(
    service: State<'_, AgentService>,
    instance_id: String,
) -> Result<(), String> {
    service.stop_instance(&instance_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn agent_get_instance(
    service: State<'_, AgentService>,
    instance_id: String,
) -> Result<InstanceInfo, String> {
    service
        .get_instance(&instance_id)
        .await
        .ok_or_else(|| "Instance not found".to_string())
}

#[tauri::command]
pub async fn agent_list_instances(service: State<'_, AgentService>) -> Result<Vec<InstanceInfo>, String> {
    Ok(service.list_instances().await)
}

#[tauri::command]
pub async fn agent_test_emit(app_handle: AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        for i in 0..5 {
            let _ = app_handle.emit("agent:test_emit", serde_json::json!({ "index": i }));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn agent_test_emit_agent_event(app_handle: AppHandle) -> Result<(), String> {
    let events = vec![
        AgentEvent::Thinking { content: "planning".to_string() },
        AgentEvent::ToolResult { tool_name: "write".to_string(), result: "Wrote file successfully.".to_string(), failed: false },
        AgentEvent::TextDelta { content: "Created `hello.py`.".to_string() },
        AgentEvent::Done,
    ];
    tokio::spawn(async move {
        for event in events {
            let payload = serde_json::json!({
                "instance_id": "test-instance",
                "event": event,
            });
            let _ = app_handle.emit("agent:event", &payload);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    Ok(())
}
