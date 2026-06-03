use super::codec::{JsonRpcRequest, JsonRpcResponse, METHOD_NOT_FOUND};
use super::connection::ConnectionHandle;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

pub struct GatewayRouter {
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
}

impl GatewayRouter {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
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
        match req.method.as_str() {
            _ => JsonRpcResponse::error(
                req.id,
                METHOD_NOT_FOUND,
                &format!("Method '{}' not found", req.method),
                None,
            ),
        }
    }

    pub fn send_to_connection(&self, conn_id: &str, msg: Message) -> Result<(), String> {
        // This is a synchronous wrapper - we'll need async in practice
        Ok(())
    }
}
