use agent_core::event_sink::EventSink;
use serde_json::Value;
use std::sync::Arc;

/// Desktop implementation of [`EventSink`] that routes events
/// over the WebSocket JSON-RPC channel.
pub struct WebSocketEventSink {
    session_manager: Arc<crate::gateway::session_manager::SessionManager>,
}

impl WebSocketEventSink {
    pub fn new(session_manager: Arc<crate::gateway::session_manager::SessionManager>) -> Self {
        Self { session_manager }
    }

    /// Extract conversation_id from payload (supports both snake_case and camelCase).
    fn extract_conversation_id(payload: &Value) -> Option<String> {
        payload
            .get("conversation_id")
            .or_else(|| payload.get("conversationId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

impl EventSink for WebSocketEventSink {
    fn emit(&self, event_type: &str, payload: Value) {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": event_type,
            "params": payload
        });

        let session_manager = self.session_manager.clone();
        let msg = tokio_tungstenite::tungstenite::Message::Text(notification.to_string());

        // Route to specific conversation if possible
        if let Some(cid) = Self::extract_conversation_id(&payload) {
            tauri::async_runtime::spawn(async move {
                let _ = session_manager.send_to_session(&cid, msg).await;
            });
            return;
        }

        // Fallback: broadcast to every connected session
        tauri::async_runtime::spawn(async move {
            session_manager.broadcast(msg).await;
        });
    }
}
