use super::codec::{JsonRpcRequest, JsonRpcResponse};
use super::router::GatewayRouter;

use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub struct ConnectionHandle {
    pub id: String,
    pub sender: mpsc::UnboundedSender<Message>,
}

impl ConnectionHandle {
    pub fn send(&self, msg: Message) {
        let _ = self.sender.send(msg);
    }
}

pub async fn handle_connection(
    conn_id: String,
    ws_stream: WebSocketStream<TcpStream>,
    router: Arc<GatewayRouter>,
) {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Spawn task to forward messages from channel to WebSocket
    let forward_id = conn_id.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
        log::debug!("WebSocket forward task ended: {}", forward_id);
    });

    let handle = ConnectionHandle {
        id: conn_id.clone(),
        sender: tx.clone(),
    };

    // Register connection with router
    router.register_connection(handle).await;

    // Process incoming messages
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                match serde_json::from_str::<JsonRpcRequest>(&text) {
                    Ok(request) => {
                        // 注册连接到 session_manager（如果请求包含 conversationId）
                        if let Some(conversation_id) = request.params.get("conversationId").and_then(|v| v.as_str()) {
                            let session_handle = crate::gateway::session_manager::ConnectionHandle {
                                id: conn_id.clone(),
                                sender: tx.clone(),
                            };
                            router.session_manager().register(conversation_id.to_string(), session_handle).await;
                        }

                        let response = router.handle_request(&conn_id, request).await;
                        if let Ok(json) = serde_json::to_string(&response) {
                            let _ = router.send_to_connection(&conn_id, Message::Text(json)).await;
                        }
                    }
                    Err(e) => {
                        let response = JsonRpcResponse::error(
                            None,
                            super::codec::PARSE_ERROR,
                            &format!("Parse error: {}", e),
                            None,
                        );
                        if let Ok(json) = serde_json::to_string(&response) {
                            let _ = router.send_to_connection(&conn_id, Message::Text(json)).await;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Unregister connection
    router.unregister_connection(&conn_id).await;
    log::info!("WebSocket connection closed: {}", conn_id);
    
    // Also unregister from session_manager to prevent stale handles
    router.session_manager().unregister_all(&conn_id).await;
}
