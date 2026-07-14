//! Fan-out [`EventSink`] that forwards events to both the WebSocket gateway
//! (for the frontend) and the platform reporter.
//!
//! Only `agent.status_changed` and `session.log` events are forwarded to
//! the reporter; all other events go to the WebSocket sink only.

use agent_core::event_sink::EventSink;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::platform::reporter::ReportEvent;

/// Event types that the reporter cares about.
const EVENT_AGENT_STATUS_CHANGED: &str = "agent.status_changed";
const EVENT_SESSION_LOG: &str = "session.log";

/// Wraps a primary [`EventSink`] (typically the WebSocket sink) and forwards
/// relevant events to the platform reporter via an mpsc channel.
pub struct ReportingEventSink {
    inner: Arc<dyn EventSink>,
    reporter_tx: mpsc::UnboundedSender<ReportEvent>,
}

impl ReportingEventSink {
    pub fn new(inner: Arc<dyn EventSink>, reporter_tx: mpsc::UnboundedSender<ReportEvent>) -> Self {
        Self {
            inner,
            reporter_tx,
        }
    }
}

impl EventSink for ReportingEventSink {
    fn emit(&self, event_type: &str, payload: Value) {
        // Always forward to the WebSocket sink first (frontend visibility).
        self.inner.emit(event_type, payload.clone());

        // Forward reporter-relevant events to the background reporter.
        let report_event = match event_type {
            EVENT_AGENT_STATUS_CHANGED => Some(ReportEvent::AgentStatusChanged(payload)),
            EVENT_SESSION_LOG => Some(ReportEvent::SessionLog(payload)),
            _ => None,
        };

        if let Some(event) = report_event {
            // UnboundedSender::send is synchronous and never blocks.
            // If the reporter has shut down, the send silently fails.
            if self.reporter_tx.send(event).is_err() {
                log::debug!(
                    "[ReportingEventSink] Reporter channel closed, event dropped"
                );
            }
        }
    }
}
