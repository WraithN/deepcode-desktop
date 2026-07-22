#![allow(dead_code)]

use crate::agui::mapper::AguiMapper;
use crate::session::SessionManager;
use agent_core::event_sink::EventSink;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Routes agent JSON-RPC notifications into AG-UI events for the right session.
pub struct AguiEventSink {
    session_manager: SessionManager,
    mappers: Arc<Mutex<HashMap<String, AguiMapper>>>,
    runtime: tokio::runtime::Handle,
}

impl AguiEventSink {
    pub fn new(session_manager: SessionManager, runtime: tokio::runtime::Handle) -> Self {
        Self {
            session_manager,
            mappers: Arc::new(Mutex::new(HashMap::new())),
            runtime,
        }
    }

    fn mapper_for(&self, instance_id: &str) -> AguiMapper {
        self.mappers
            .lock()
            .unwrap()
            .entry(instance_id.to_string())
            .or_default()
            .clone()
    }

    fn update_mapper(&self, instance_id: &str, mapper: AguiMapper) {
        self.mappers
            .lock()
            .unwrap()
            .insert(instance_id.to_string(), mapper);
    }
}

impl EventSink for AguiEventSink {
    fn emit(&self, event_type: &str, payload: Value) {
        let instance_id = payload
            .get("instance_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let conversation_id = payload
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut mapper = self.mapper_for(&instance_id);
        let events = mapper.map(event_type, &payload);
        self.update_mapper(&instance_id, mapper);

        let session_manager = self.session_manager.clone();
        let event_type = event_type.to_string();
        self.runtime.spawn(async move {
            let log_conversation_id = conversation_id.clone();
            let session_id = if conversation_id.is_empty() {
                match session_manager.session_for_instance(&instance_id).await {
                    Some(sid) => sid,
                    None => {
                        tracing::warn!(
                            "[agui-sink] DROPPING event={event_type} — no session for instance_id={instance_id}"
                        );
                        return;
                    }
                }
            } else {
                conversation_id
            };

            tracing::info!(
                "[agui-sink] event={event_type} instance={instance_id} conv_id={log_conversation_id} routing_to={session_id}"
            );

            for event in events {
                session_manager.broadcast(&session_id, event).await;
            }
        });
    }
}
