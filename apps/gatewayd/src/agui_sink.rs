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
}

impl AguiEventSink {
    pub fn new(session_manager: SessionManager) -> Self {
        Self {
            session_manager,
            mappers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn mapper_for(&self, instance_id: &str) -> AguiMapper {
        self.mappers
            .lock()
            .unwrap()
            .entry(instance_id.to_string())
            .or_insert_with(AguiMapper::new)
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

        let Some(session_id) = self.session_manager.session_for_instance(&instance_id) else {
            return;
        };

        let mut mapper = self.mapper_for(&instance_id);
        let events = mapper.map(event_type, &payload);
        self.update_mapper(&instance_id, mapper);

        for event in events {
            self.session_manager.broadcast(&session_id, event);
        }
    }
}
