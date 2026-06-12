use serde_json::Value;
use std::sync::Arc;

/// Abstraction over event delivery mechanisms.
///
/// Desktop uses WebSocket JSON-RPC broadcast; gatewayd may use
/// HTTP SSE or an in-memory channel.  All agent events and session
/// logs flow through this trait so that `agent-core` remains
/// decoupled from Tauri.
pub trait EventSink: Send + Sync {
    /// Emit an event.
    ///
    /// * `event_type` – frontend-facing method name, e.g.
    ///   `"agent.event"`, `"agent.status_changed"`, `"session.log"`.
    /// * `payload`    – arbitrary JSON payload.
    fn emit(&self, event_type: &str, payload: Value);
}

/// Type-erased handle to an [`EventSink`].
pub type DynEventSink = Arc<dyn EventSink>;
