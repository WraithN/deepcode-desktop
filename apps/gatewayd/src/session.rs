#![allow(dead_code)]

use crate::agui::types::{BaseEvent, Event, Message, RunAgentInput};
use agent_core::error::InstanceError;
use agent_core::models::CreateInstanceRequest;
use agent_core::service::AgentService;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

const DEFAULT_BROADCAST_CAPACITY: usize = 1024;

/// Errors that can occur when starting a run.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("session not found")]
    SessionNotFound,
    #[error("no agent instance in session")]
    NoAgent,
    #[error("no user message found")]
    NoUserMessage,
    #[error("session already has an agent instance")]
    InstanceAlreadyExists,
    #[error("agent error: {0}")]
    AgentError(#[from] InstanceError),
}

/// A single AG-UI session.  Holds the event broadcaster and the list of
/// attached agent instances.
#[derive(Clone)]
pub struct Session {
    pub session_id: String,
    pub event_tx: broadcast::Sender<Event>,
    instances: Arc<Mutex<Vec<String>>>,
    state: Arc<Mutex<Value>>,
}

impl Session {
    fn new(session_id: String) -> Self {
        let (event_tx, _rx) = broadcast::channel(DEFAULT_BROADCAST_CAPACITY);
        Self {
            session_id,
            event_tx,
            instances: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(Value::Object(serde_json::Map::new()))),
        }
    }

    pub fn add_instance(&self, instance_id: String) {
        self.instances.lock().unwrap().push(instance_id);
    }

    pub fn instances(&self) -> Vec<String> {
        self.instances.lock().unwrap().clone()
    }

    pub fn state(&self) -> Value {
        self.state.lock().unwrap().clone()
    }

    pub fn set_state(&self, state: Value) {
        *self.state.lock().unwrap() = state;
    }
}

/// Manages AG-UI sessions and routes agent events to the right session.
#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new session and return its id.
    pub fn create_session(&self) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(session_id.clone());
        self.inner
            .lock()
            .unwrap()
            .insert(session_id.clone(), session);
        session_id
    }

    /// Get a session by id.
    pub fn get_session(&self, session_id: &str) -> Option<Session> {
        self.inner.lock().unwrap().get(session_id).cloned()
    }

    /// Create an agent instance under the given session.
    pub async fn create_agent(
        &self,
        session_id: &str,
        plugin_key: &str,
        name: &str,
        workspace: &str,
        force: bool,
        agent_service: &AgentService,
    ) -> Result<agent_core::models::InstanceInfo, agent_core::error::PluginError> {
        let session = self.get_session(session_id).ok_or_else(|| {
            agent_core::error::PluginError::NotFound(format!("session {session_id}"))
        })?;

        if !session.instances().is_empty() && !force {
            // We cannot return a custom error here because AgentService uses PluginError.
            // Use CreateInstanceFailed as a close match.
            return Err(agent_core::error::PluginError::CreateInstanceFailed(
                "session already has an agent instance".to_string(),
            ));
        }

        let req = CreateInstanceRequest {
            plugin_key: plugin_key.to_string(),
            name: name.to_string(),
            workspace: workspace.to_string(),
            force,
        };

        let info = agent_service.create_instance(req).await?;
        session.add_instance(info.id.clone());
        Ok(info)
    }

    /// Start a run for the given session using the provided input.
    pub async fn start_run(
        &self,
        session_id: &str,
        input: RunAgentInput,
        agent_service: &AgentService,
    ) -> Result<String, RunError> {
        let session = self
            .get_session(session_id)
            .ok_or(RunError::SessionNotFound)?;

        let instances = session.instances();
        if instances.is_empty() {
            return Err(RunError::NoAgent);
        }

        let run_id = input
            .run_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let _ = session.event_tx.send(Event::RunStarted {
            base: BaseEvent {
                timestamp: Some(now()),
                raw_event: None,
            },
            thread_id: session_id.to_string(),
            run_id: run_id.clone(),
        });

        session.set_state(input.state.clone());
        let _ = session.event_tx.send(Event::StateSnapshot {
            base: BaseEvent {
                timestamp: Some(now()),
                raw_event: None,
            },
            snapshot: input.state,
        });

        let instance_id = instances.first().cloned().unwrap();
        let message = input
            .messages
            .into_iter()
            .rev()
            .find(|m| matches!(m, Message::User { .. }))
            .and_then(|m| m.content().map(|s| s.to_string()))
            .ok_or(RunError::NoUserMessage)?;

        agent_service
            .send_message(&instance_id, session_id, &message)
            .await
            .map_err(RunError::AgentError)?;

        Ok(run_id)
    }

    /// Subscribe to events for a session.
    pub fn subscribe(&self, session_id: &str) -> Option<broadcast::Receiver<Event>> {
        self.get_session(session_id).map(|s| s.event_tx.subscribe())
    }

    /// Broadcast an event to all subscribers of a session.
    pub fn broadcast(&self, session_id: &str, event: Event) {
        if let Some(session) = self.get_session(session_id) {
            let _ = session.event_tx.send(event);
        }
    }

    /// Find the session id that owns the given instance.
    pub fn session_for_instance(&self, instance_id: &str) -> Option<String> {
        let guard = self.inner.lock().unwrap();
        for (sid, session) in guard.iter() {
            if session.instances().contains(&instance_id.to_string()) {
                return Some(sid.clone());
            }
        }
        None
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
