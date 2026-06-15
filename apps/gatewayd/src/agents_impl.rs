use agent_core::error::{InstanceError, PluginError};
use agent_core::event_sink::{DynEventSink, EventSink};
use agent_core::instance::{AgentInstance, InstanceConfig};
use agent_core::logger::SessionLogger;
use agent_core::models::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use axum::{
    extract::{Path, Query, State},
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{AgentEvent, ApiState};

pub struct GatewaydEventSink {
    broadcaster: tokio::sync::broadcast::Sender<AgentEvent>,
}

impl GatewaydEventSink {
    pub fn new(broadcaster: tokio::sync::broadcast::Sender<AgentEvent>) -> Self {
        Self { broadcaster }
    }
}

impl EventSink for GatewaydEventSink {
    fn emit(&self, event_type: &str, payload: serde_json::Value) {
        tracing::info!("[agent-event] {}: {}", event_type, payload);
        let instance_id = payload
            .get("instance_id")
            .or_else(|| payload.get("sessionID"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let event = AgentEvent {
            event_type: event_type.to_string(),
            instance_id,
            payload,
        };
        let _ = self.broadcaster.send(event);
    }
}

pub fn init_agent_service(
    event_broadcaster: tokio::sync::broadcast::Sender<AgentEvent>,
) -> Result<AgentService, anyhow::Error> {
    let event_sink = Arc::new(GatewaydEventSink::new(event_broadcaster));
    let data_dir = dh_platform::fs::data_dir()?;
    let db_path = data_dir.join("agent_logs.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    let log_file = data_dir.join("agent.log");
    let logger = Arc::new(SessionLogger::new(event_sink.clone(), conn, Some(log_file)));
    let mut agent_service = AgentService::new(logger.clone(), event_sink.clone());
    agent_service.register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(logger)));
    Ok(agent_service)
}

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn agent_core::plugin::AgentPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }
    pub fn register(&mut self, plugin: Box<dyn agent_core::plugin::AgentPlugin>) {
        self.plugins.insert(plugin.key().to_string(), plugin);
    }
    pub fn get(&self, key: &str) -> Option<&Box<dyn agent_core::plugin::AgentPlugin>> {
        self.plugins.get(key)
    }
    pub fn list(&self) -> Vec<(&String, &Box<dyn agent_core::plugin::AgentPlugin>)> {
        self.plugins.iter().collect()
    }
}

pub struct InstanceRegistry {
    instances: HashMap<String, Arc<dyn AgentInstance>>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }
    pub fn insert(&mut self, id: String, instance: Arc<dyn AgentInstance>) {
        self.instances.insert(id, instance);
    }
    pub fn get(&self, id: &str) -> Option<Arc<dyn AgentInstance>> {
        self.instances.get(id).cloned()
    }
    pub fn remove(&mut self, id: &str) {
        self.instances.remove(id);
    }
    pub fn list(&self) -> Vec<(&String, &Arc<dyn AgentInstance>)> {
        self.instances.iter().collect()
    }
}

pub struct AgentService {
    plugins: PluginRegistry,
    instances: Arc<Mutex<InstanceRegistry>>,
    logger: Arc<SessionLogger>,
    event_sink: DynEventSink,
}

impl AgentService {
    pub fn new(logger: Arc<SessionLogger>, event_sink: DynEventSink) -> Self {
        Self {
            plugins: PluginRegistry::new(),
            instances: Arc::new(Mutex::new(InstanceRegistry::new())),
            logger,
            event_sink,
        }
    }
    pub fn register_plugin(&mut self, plugin: Box<dyn agent_core::plugin::AgentPlugin>) {
        self.plugins.register(plugin);
    }
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .list()
            .into_iter()
            .map(|(key, p)| PluginInfo {
                key: key.clone(),
                name: p.name().to_string(),
                installed: p.is_installed(),
            })
            .collect()
    }
    pub async fn create_instance(
        &self,
        req: CreateInstanceRequest,
    ) -> Result<InstanceInfo, PluginError> {
        let plugin = self
            .plugins
            .get(&req.plugin_key)
            .ok_or(PluginError::NotFound(req.plugin_key.clone()))?;
        let id = format!("{}-{}", req.plugin_key, uuid::Uuid::new_v4());
        let config = InstanceConfig::new(id.clone(), req.name.clone(), req.workspace.clone());
        let instance = plugin.create_instance(config, self.event_sink.clone())?;
        let info = InstanceInfo {
            id: instance.id().to_string(),
            plugin_key: req.plugin_key.clone(),
            name: req.name.clone(),
            workspace: req.workspace.clone(),
            status: instance.status(),
            endpoint: instance.endpoint(),
        };
        self.instances.lock().await.insert(id, Arc::from(instance));
        Ok(info)
    }
    pub async fn send_message(
        &self,
        instance_id: &str,
        conversation_id: &str,
        message: &str,
    ) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;
        let message = message.to_string();
        let conversation_id = conversation_id.to_string();
        tokio::spawn(async move {
            let _ = instance.send_message(&conversation_id, &message).await;
        });
        Ok(())
    }
    pub async fn stop_instance(&self, instance_id: &str) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;
        instance.stop().await
    }
    pub async fn get_instance(&self, instance_id: &str) -> Option<InstanceInfo> {
        let registry = self.instances.lock().await;
        let instance = registry.get(instance_id)?;
        Some(InstanceInfo {
            id: instance.id().to_string(),
            plugin_key: instance.plugin_key().to_string(),
            name: instance.id().to_string(),
            workspace: "".to_string(),
            status: instance.status(),
            endpoint: instance.endpoint(),
        })
    }
    pub async fn list_instances(&self) -> Vec<InstanceInfo> {
        let registry = self.instances.lock().await;
        registry
            .list()
            .into_iter()
            .map(|(id, instance)| InstanceInfo {
                id: id.clone(),
                plugin_key: instance.plugin_key().to_string(),
                name: instance.id().to_string(),
                workspace: "".to_string(),
                status: instance.status(),
                endpoint: instance.endpoint(),
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub plugin_type: String,
    pub name: String,
    pub workspace: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub conversation_id: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub instance_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    instance_id: Option<String>,
}

pub async fn create_agent_handler(
    State(state): State<ApiState>,
    Json(req): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(s) => s,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    let create_req = agent_core::models::CreateInstanceRequest {
        plugin_key: req.plugin_type.clone(),
        name: req.name.clone(),
        workspace: req.workspace.clone(),
    };
    match service.create_instance(create_req).await {
        Ok(info) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "instance_id": info.id,
                "plugin_key": info.plugin_key,
                "name": info.name,
                "status": info.status,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn list_agents_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(s) => s,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    let instances = service.list_instances().await;
    (StatusCode::OK, Json(instances)).into_response()
}

pub async fn get_agent_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(s) => s,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    match service.get_instance(&id).await {
        Some(info) => (StatusCode::OK, Json(info)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "instance not found"}))).into_response(),
    }
}

pub async fn send_message_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(s) => s,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    match service
        .send_message(&id, &req.conversation_id, &req.message)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "sent"}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn stop_agent_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(s) => s,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    match service.stop_instance(&id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "stopped"}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn events_handler(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    Query(query): Query<EventsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_events_socket(socket, state, query.instance_id))
}

async fn handle_events_socket(
    socket: WebSocket,
    state: ApiState,
    filter_instance_id: Option<String>,
) {
    let mut rx = state.event_broadcaster.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Forward broadcast events to the WebSocket client.
    let forward_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Some(ref filter) = filter_instance_id {
                if event.instance_id.as_ref() != Some(filter) {
                    continue;
                }
            }
            let msg = serde_json::to_string(&event).unwrap_or_default();
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Keep the socket alive while the client is connected.
    while let Some(Ok(_msg)) = receiver.next().await {}

    forward_task.abort();
}
