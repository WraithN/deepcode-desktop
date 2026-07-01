#![allow(dead_code)]

use crate::agui::types::{BaseEvent, Event, Message, RunAgentInput};
use agent_core::error::InstanceError;
use agent_core::models::CreateInstanceRequest;
use agent_core::service::AgentService;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use uuid;

const DEFAULT_BROADCAST_CAPACITY: usize = 1024;
const DEFAULT_EXPIRED_TIME_SECS: u64 = 600;

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

/// A single AG-UI session.  Holds the event broadcaster, the list of
/// attached agent instances, and idle-timeout metadata for reaping.
#[derive(Clone)]
pub struct Session {
    pub session_id: String,
    pub event_tx: broadcast::Sender<Event>,
    instances: Arc<Mutex<Vec<String>>>,
    state: Arc<Mutex<Value>>,
    expired_time: Duration,
    last_input_at: Arc<Mutex<Instant>>,
}

impl Session {
    fn new(session_id: String, expired_time: Duration) -> Self {
        let (event_tx, _rx) = broadcast::channel(DEFAULT_BROADCAST_CAPACITY);
        Self {
            session_id,
            event_tx,
            instances: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(Value::Object(serde_json::Map::new()))),
            expired_time,
            last_input_at: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub fn add_instance(&self, instance_id: String) {
        self.instances.lock().unwrap().push(instance_id);
    }

    pub fn instances(&self) -> Vec<String> {
        self.instances.lock().unwrap().clone()
    }

    pub fn clear_instances(&self) {
        self.instances.lock().unwrap().clear();
    }

    pub fn state(&self) -> Value {
        self.state.lock().unwrap().clone()
    }

    pub fn set_state(&self, state: Value) {
        *self.state.lock().unwrap() = state;
    }

    /// 更新最近一次用户输入时间，用于空闲回收判定。
    pub fn touch(&self) {
        *self.last_input_at.lock().unwrap() = Instant::now();
    }

    /// 判断 session 是否已超过 expired_time 没有用户输入。
    pub fn is_expired(&self) -> bool {
        let last = *self.last_input_at.lock().unwrap();
        Instant::now().duration_since(last) > self.expired_time
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
    /// `expired_time_secs` 为空闲超时秒数，None 则使用默认值 600。
    pub fn create_session(&self, expired_time_secs: Option<u64>) -> String {
        let secs = expired_time_secs.unwrap_or(DEFAULT_EXPIRED_TIME_SECS);
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(session_id.clone(), Duration::from_secs(secs));
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

    /// 更新 session 的最近用户输入时间。
    pub fn touch_session(&self, session_id: &str) {
        if let Some(session) = self.get_session(session_id) {
            session.touch();
        }
    }

    /// Create an agent instance under the given session.
    pub async fn create_agent(
        &self,
        session_id: &str,
        agent_key: &str,
        name: &str,
        work_directory: &str,
        force: bool,
        agent_service: &AgentService,
    ) -> Result<agent_core::models::InstanceInfo, agent_core::error::PluginError> {
        let session = self.get_session(session_id).ok_or_else(|| {
            agent_core::error::PluginError::NotFound(format!("session {session_id}"))
        })?;

        if !session.instances().is_empty() && !force {
            return Err(agent_core::error::PluginError::CreateInstanceFailed(
                "session already has an agent instance".to_string(),
            ));
        }

        let req = CreateInstanceRequest {
            agent_key: agent_key.to_string(),
            name: name.to_string(),
            work_directory: work_directory.to_string(),
            force,
        };

        let info = agent_service.create_instance(req).await?;
        session.add_instance(info.id.clone());
        Ok(info)
    }

    /// 如果 session 没有 agent 实例且 run 请求携带了 agent_key，则自动挂载对应插件。
    /// 将挂载逻辑提取为小函数，避免 start_run 出现过深嵌套。
    async fn ensure_agent_for_run(
        &self,
        session_id: &str,
        agent_key: &str,
        run_id: &str,
        agent_service: &AgentService,
    ) -> Result<(), RunError> {
        let work_directory = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        tracing::info!(
            "[session_manager] run={} session={} has no agent, auto-attaching agent_key={}",
            run_id,
            session_id,
            agent_key
        );
        match self
            .create_agent(
                session_id,
                agent_key,
                &format!("{}-auto", agent_key),
                &work_directory,
                false,
                agent_service,
            )
            .await
        {
            Ok(info) => {
                tracing::info!(
                    "[session_manager] run={} auto-attached instance={} agent_key={}",
                    run_id,
                    info.id,
                    info.agent_key
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    "[session_manager] run={} auto-attach agent_key={} failed: {}",
                    run_id,
                    agent_key,
                    e
                );
                Err(RunError::NoAgent)
            }
        }
    }

    /// Start a run for the given session using the provided input.
    pub async fn start_run(
        &self,
        session_id: &str,
        input: RunAgentInput,
        agent_service: &AgentService,
    ) -> Result<String, RunError> {
        let run_id = input
            .run_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let start = std::time::Instant::now();
        tracing::info!(
            "[session_manager] run={} start_run begin for session={}",
            run_id,
            session_id
        );

        let session = self
            .get_session(session_id)
            .ok_or(RunError::SessionNotFound)?;

        // 收到用户输入，刷新空闲计时器，防止 session 被回收。
        session.touch();

        let mut instances = session.instances();

        // 如果 session 尚未挂载 agent，且 run 请求携带了 agent_key，自动加载对应 agent。
        if instances.is_empty() {
            if let Some(agent_key) = input.agent_key.as_deref().filter(|s| !s.is_empty()) {
                self.ensure_agent_for_run(session_id, agent_key, &run_id, agent_service)
                    .await?;
                instances = session.instances();
            }
        }

        if instances.is_empty() {
            return Err(RunError::NoAgent);
        }

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

        tracing::info!(
            "[session_manager] run={} sending user message to instance={} after {:?}",
            run_id,
            instance_id,
            start.elapsed()
        );

        let send_start = std::time::Instant::now();
        agent_service
            .send_message(&instance_id, session_id, &message)
            .await
            .map_err(RunError::AgentError)?;
        tracing::info!(
            "[session_manager] run={} agent_service.send_message returned after {:?}",
            run_id,
            send_start.elapsed()
        );

        tracing::info!(
            "[session_manager] run={} start_run completed after {:?}",
            run_id,
            start.elapsed()
        );

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

    /// 遍历所有 session，回收超过 expired_time 无用户输入的实例。
    /// 回收操作会停止底层进程并从 session 与 AgentService 注册表中移除实例。
    pub async fn reap_expired(&self, agent_service: &AgentService) {
        let expired: Vec<(String, Vec<String>)> = {
            let guard = self.inner.lock().unwrap();
            guard
                .iter()
                .filter(|(_, session)| session.is_expired() && !session.instances().is_empty())
                .map(|(sid, session)| (sid.clone(), session.instances()))
                .collect()
        };

        for (session_id, instance_ids) in expired {
            for instance_id in &instance_ids {
                tracing::info!(
                    "[session_manager] reaping expired instance={} session={}",
                    instance_id,
                    session_id
                );
                if let Err(e) = agent_service.stop_and_remove_instance(instance_id).await {
                    tracing::warn!(
                        "[session_manager] failed to stop instance={}: {}",
                        instance_id,
                        e
                    );
                }
            }
            if let Some(session) = self.get_session(&session_id) {
                session.clear_instances();
            }
        }
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
