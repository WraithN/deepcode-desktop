use super::codec::{JsonRpcRequest, JsonRpcResponse, METHOD_NOT_FOUND};
use super::connection::ConnectionHandle;
use super::handlers::agent::handle_agent_request;
use super::handlers::session::handle_session_request;
use crate::service::agent_service::AgentService;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

pub struct GatewayRouter {
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
    agent_service: Arc<AgentService>,
}

impl GatewayRouter {
    pub fn new(agent_service: Arc<AgentService>) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            agent_service,
        }
    }
    
    pub async fn register_connection(&self, handle: ConnectionHandle) {
        let mut conns = self.connections.write().await;
        conns.insert(handle.id.clone(), handle);
    }
    
    pub async fn unregister_connection(&self, conn_id: &str) {
        let mut conns = self.connections.write().await;
        conns.remove(conn_id);
    }
    
    pub async fn handle_request(&self, _conn_id: &str, req: JsonRpcRequest) -> JsonRpcResponse {
        if req.method.starts_with("agent.") {
            handle_agent_request(self.agent_service.clone(), req).await
        } else if req.method.starts_with("session.") {
            handle_session_request(req).await
        } else {
            JsonRpcResponse::error(
                req.id,
                METHOD_NOT_FOUND,
                &format!("Method '{}' not found", req.method),
                None,
            )
        }
    }
    
    pub async fn broadcast(&self, message: String) {
        let conns = self.connections.read().await;
        for (_, handle) in conns.iter() {
            let _ = handle.sender.send(Message::Text(message.clone()));
        }
    }
    
    pub fn send_to_connection(&self, _conn_id: &str, _msg: Message) -> Result<(), String> {
        // TODO: Implement sync wrapper or change architecture
        Ok(())
    }
}
