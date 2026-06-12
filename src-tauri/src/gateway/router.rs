use super::codec::{JsonRpcRequest, JsonRpcResponse, METHOD_NOT_FOUND};
use super::connection::ConnectionHandle;
use super::handlers::agent::handle_agent_request;
use super::handlers::db::handle_db_request;
use super::handlers::session::handle_session_request;
use super::session_manager::SessionManager;
use crate::service::agent_service::AgentService;
use crate::service::db_service::DbService;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

pub struct GatewayRouter {
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
    agent_service: Arc<AgentService>,
    db_service: Arc<DbService>,

    session_manager: Arc<SessionManager>,
}

impl GatewayRouter {
    pub fn new(
        agent_service: Arc<AgentService>,
        db_service: Arc<DbService>,
        session_manager: Arc<SessionManager>,
    ) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            agent_service,
            db_service,
            session_manager,
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
            handle_agent_request(
                self.agent_service.clone(),
                self.session_manager.clone(),
                req,
            ).await
        } else if req.method.starts_with("session.") {
            handle_session_request(req, self.db_service.clone()).await
        } else if req.method.starts_with("db.") {
            handle_db_request(self.db_service.clone(), req).await
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

    pub async fn send_to_connection(&self, conn_id: &str, msg: Message) -> Result<(), String> {
        let conns = self.connections.read().await;
        let handle = conns.get(conn_id).ok_or_else(|| format!("Connection {} not found", conn_id))?;
        handle.sender.send(msg).map_err(|e| e.to_string())
    }

    pub fn session_manager(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }



    pub fn agent_service(&self) -> Arc<AgentService> {
        self.agent_service.clone()
    }
}
