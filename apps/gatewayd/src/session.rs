#![allow(dead_code)]

use crate::agui::types::{BaseEvent, Event, Message, RunAgentInput};
use agent_core::error::InstanceError;
use agent_core::error::PluginError;
use agent_core::models::CreateInstanceRequest;
use agent_core::service::AgentService;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::sync::broadcast;

const DEFAULT_BROADCAST_CAPACITY: usize = 1024;
const DEFAULT_EXPIRED_TIME_SECS: u64 = 600;
const INSTANCE_SHUTDOWN_TIMEOUT_SECS: u64 = 10;
const SESSIONS_FILE: &str = "sessions.json";

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
    #[error("session already has an active run")]
    RunAlreadyActive,
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
    /// 标记当前是否有 run 正在执行。Arc 共享保证 Session 被 clone 后
    /// （如 reaper 通过 inner map 读取、start_run 通过 get_session 读取）
    /// 看到的是同一个标志位。
    run_active: Arc<AtomicBool>,
    /// 当前在途 run 的 ID（None 表示无 run 在途）。由 start_run 置位，
    /// 由事件消费者在收到 agent.done / agent.error 终态时复位，
    /// 用于让消费者补发的 RUN_FINISHED 携带正确的 run_id。
    current_run_id: Arc<Mutex<Option<String>>>,
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
            run_active: Arc::new(AtomicBool::new(false)),
            current_run_id: Arc::new(Mutex::new(None)),
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

    /// 标记一个 run 开始并记录其 run_id。
    /// AG-UI 同一 thread 同一时间只允许一个 run 在途：已有 run 时返回 false，
    /// 调用方应拒绝本次请求，否则回合结束信号（agent.done）会被错误归属。
    pub fn begin_run(&self, run_id: String) -> bool {
        let mut guard = self.current_run_id.lock().unwrap();
        if guard.is_some() {
            return false;
        }
        *guard = Some(run_id);
        self.run_active.store(true, Ordering::SeqCst);
        true
    }

    /// 结束当前在途 run 并返回其 run_id；无在途 run 时返回 None。
    pub fn end_run(&self) -> Option<String> {
        let run_id = self.current_run_id.lock().unwrap().take();
        if run_id.is_some() {
            self.run_active.store(false, Ordering::SeqCst);
        }
        run_id
    }

    /// 判断 session 是否已超过 expired_time 没有用户输入。
    /// run 执行期间不视为过期，避免 reaper 在长时间 run 中途杀掉 agent 进程。
    pub fn is_expired(&self) -> bool {
        if self.run_active.load(Ordering::SeqCst) {
            return false;
        }
        let last = *self.last_input_at.lock().unwrap();
        Instant::now().duration_since(last) > self.expired_time
    }
}

/// 持久化到 sessions.json 的单条记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub session_id: String,
    pub workspace_path: String,
    pub last_used: u64,
}

/// On-disk representation of the sessions persistence file.  The version field
/// allows us to evolve the schema and detect legacy files in the future.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionsFile {
    version: u32,
    entries: Vec<WorkspaceEntry>,
}

impl SessionsFile {
    const CURRENT_VERSION: u32 = 1;

    fn new(entries: Vec<WorkspaceEntry>) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            entries,
        }
    }
}

/// 获取数据存储目录：优先使用 GATEWAYD_DATA_DIR 环境变量，否则使用 ~/.dh-gatewayd。
fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("GATEWAYD_DATA_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".dh-gatewayd")
}

fn sessions_file_path() -> PathBuf {
    data_dir().join(SESSIONS_FILE)
}

fn save_workspaces(workspaces: &HashMap<String, WorkspaceEntry>) {
    let path = sessions_file_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::error!("[session_manager] failed to create data dir: {}", e);
            return;
        }
    }

    let file = SessionsFile::new(workspaces.values().cloned().collect());
    let json = match serde_json::to_string_pretty(&file) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("[session_manager] failed to serialize sessions: {}", e);
            return;
        }
    };

    // Write to a temporary file and rename atomically so the on-disk state is
    // never a partially written JSON file.
    let temp_path = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&temp_path, json) {
        tracing::error!("[session_manager] failed to write sessions temp file: {}", e);
        return;
    }
    if let Err(e) = std::fs::rename(&temp_path, &path) {
        tracing::error!("[session_manager] failed to persist sessions file: {}", e);
        let _ = std::fs::remove_file(&temp_path);
    }
}

fn load_workspaces() -> HashMap<String, WorkspaceEntry> {
    let path = sessions_file_path();
    if !path.exists() {
        return HashMap::new();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("[session_manager] failed to read sessions file: {}", e);
            return HashMap::new();
        }
    };

    // First try the new versioned format...
    if let Ok(file) = serde_json::from_str::<SessionsFile>(&content) {
        if file.version != SessionsFile::CURRENT_VERSION {
            tracing::warn!(
                "[session_manager] sessions file version {} does not match current {}, resetting",
                file.version,
                SessionsFile::CURRENT_VERSION
            );
            return HashMap::new();
        }
        return file
            .entries
            .into_iter()
            .map(|e| (e.session_id.clone(), e))
            .collect();
    }

    // ...then fall back to the legacy flat-array format for backwards compatibility.
    match serde_json::from_str::<Vec<WorkspaceEntry>>(&content) {
        Ok(entries) => entries.into_iter().map(|e| (e.session_id.clone(), e)).collect(),
        Err(e) => {
            tracing::error!("[session_manager] failed to parse sessions file: {}", e);
            HashMap::new()
        }
    }
}

/// Manages AG-UI sessions and routes agent events to the right session.
#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<RwLock<HashMap<String, Session>>>,
    workspaces: Arc<RwLock<HashMap<String, WorkspaceEntry>>>,
    /// Optional readiness gate. When present, `create_agent` is held
    /// until the platform has confirmed a `workspacePath` for this
    /// runtime, and an empty caller-provided `work_directory` is
    /// resolved to the platform-assigned path.
    readiness: Option<Arc<crate::readiness::WorkspacePathReadiness>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Creates a session manager in local mode (no readiness gate).
    pub fn new() -> Self {
        let workspaces = load_workspaces();
        let mut inner = HashMap::new();
        // 从持久化记录恢复 Session（仅创建空 Session，不启动 agent）
        for sid in workspaces.keys() {
            inner.insert(
                sid.clone(),
                Session::new(sid.clone(), Duration::from_secs(DEFAULT_EXPIRED_TIME_SECS)),
            );
        }
        tracing::info!(
            "[session_manager] loaded {} persisted sessions",
            workspaces.len()
        );
        Self {
            inner: Arc::new(RwLock::new(inner)),
            workspaces: Arc::new(RwLock::new(workspaces)),
            readiness: None,
        }
    }

    /// Creates a session manager wired to a platform readiness gate.
    ///
    /// In platform mode `create_agent` is held until the first successful
    /// status sync, and an empty caller-provided `work_directory` is
    /// resolved to the platform-assigned path before the agent starts.
    pub fn with_readiness(readiness: Arc<crate::readiness::WorkspacePathReadiness>) -> Self {
        let mut s = Self::new();
        s.readiness = Some(readiness);
        s
    }

    /// Returns the platform-assigned workspace path, if the readiness
    /// gate is open. Returns `None` in local mode or before the first
    /// successful sync.
    pub fn platform_workspace_path(&self) -> Option<String> {
        self.readiness.as_ref()?.current_path()
    }

    /// Create a new session and return its id.
    /// `preferred_id` 若提供且不与已有 session 冲突，则直接使用该 ID；
    /// 若冲突（session 已存在）则直接返回已有 session 的 ID（幂等）。
    /// `expired_time_secs` 为空闲超时秒数，None 则使用默认值 600。
    pub async fn create_session(
        &self,
        preferred_id: Option<String>,
        expired_time_secs: Option<u64>,
    ) -> String {
        if let Some(ref pid) = preferred_id {
            let guard = self.inner.read().await;
            if guard.contains_key(pid) {
                return pid.clone();
            }
        }
        let session_id = preferred_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let secs = expired_time_secs.unwrap_or(DEFAULT_EXPIRED_TIME_SECS);
        let session = Session::new(session_id.clone(), Duration::from_secs(secs));
        self.inner
            .write()
            .await
            .insert(session_id.clone(), session);
        session_id
    }

    /// Get a session by id.
    pub async fn get_session(&self, session_id: &str) -> Option<Session> {
        self.inner.read().await.get(session_id).cloned()
    }

    /// 更新 session 的最近用户输入时间。
    pub async fn touch_session(&self, session_id: &str) {
        if let Some(session) = self.get_session(session_id).await {
            session.touch();
        }
    }

    /// Create an agent instance under the given session.
    ///
    /// If the session already has an agent instance and `force` is false, the
    /// requested agent key / workspace / name must match the existing instance.
    /// This keeps the session-level semantics aligned with `AgentService` reuse
    /// rules and prevents silently attaching a mismatched agent to a session.
    ///
    /// The workspace mapping is persisted before creating the instance so that a
    /// failure can be rolled back; the instance is stopped and removed if the
    /// session disappears before the instance can be attached.
    ///
    /// ## Readiness gate (platform mode)
    ///
    /// When a `WorkspacePathReadiness` is wired in (i.e. platform reporting is
    /// configured):
    /// - `create_agent` is **held** until the gate opens. Until then the
    ///   caller gets `PluginError::CreateInstanceFailed` with a clear
    ///   "waiting for platform sync" message.
    /// - An empty `work_directory` (`""`) is resolved to the
    ///   platform-assigned path. This is the recommended way to call us from
    ///   prewarm / auto-attach flows so the runtime always picks up the
    ///   canonical path.
    pub async fn create_agent(
        &self,
        session_id: &str,
        agent_key: &str,
        name: &str,
        work_directory: &str,
        force: bool,
        agent_service: &AgentService,
    ) -> Result<agent_core::models::InstanceInfo, PluginError> {
        // Readiness gate: hold the call until platform has confirmed a
        // workspace path. The caller gets a clear "wait" error so the UI
        // can surface a friendly message instead of starting an agent
        // against a stale or missing sandbox.
        if let Some(readiness) = &self.readiness {
            if !readiness.is_ready() {
                return Err(PluginError::CreateInstanceFailed(
                    "workspace_path not ready: waiting for platform to confirm a workspace path. \
                     请稍候再试，或检查 platform 服务可达性。"
                        .to_string(),
                ));
            }
        }

        // Resolve an empty caller-provided work_directory:
        // 1. If a readiness gate is wired and is open, use the
        //    platform-assigned path.
        // 2. Otherwise fall back to the process CWD.
        // This keeps prewarm and auto-attach flows working without hard-
        // coding a path, and ensures new agent instances always run inside
        // the platform-assigned sandbox when one is available.
        let resolved_work_dir: String = if work_directory.trim().is_empty() {
            if let Some(path) = self.platform_workspace_path() {
                path
            } else {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            }
        } else {
            work_directory.to_string()
        };

        let work_directory = resolved_work_dir.as_str();

        // Check whether the session already has an instance that we can reuse.
        let existing_id = {
            let guard = self.inner.read().await;
            let session = guard.get(session_id).ok_or_else(|| {
                PluginError::NotFound(format!("session {session_id}"))
            })?;
            let instances = session.instances();
            if !instances.is_empty() && !force {
                Some(instances.first().unwrap().clone())
            } else {
                None
            }
        };

        if let Some(existing_id) = existing_id {
            let existing = agent_service
                .get_instance(&existing_id)
                .await
                .ok_or_else(|| {
                    PluginError::CreateInstanceFailed(
                        "existing instance not found in agent service".to_string(),
                    )
                })?;
            if existing.agent_key == agent_key
                && existing.work_directory == work_directory
                && existing.name == name
            {
                return Ok(existing);
            }
            return Err(PluginError::CreateInstanceFailed(
                "session already has an agent instance with different config".to_string(),
            ));
        }

        // Persist the workspace mapping first. If instance creation fails we can
        // roll this back so the on-disk state stays consistent with the runtime.
        self.persist_workspace(session_id, work_directory).await;

        let req = CreateInstanceRequest {
            agent_key: agent_key.to_string(),
            name: name.to_string(),
            work_directory: work_directory.to_string(),
            force,
        };

        let info = match agent_service.create_instance(req).await {
            Ok(info) => info,
            Err(e) => {
                self.remove_workspace(session_id).await;
                return Err(e);
            }
        };

        // Attach the instance to the session. If the session has disappeared,
        // stop the newly created instance and roll back the persistence.
        {
            let guard = self.inner.write().await;
            if let Some(session) = guard.get(session_id) {
                // 新实例创建后，移除 session 中原有的旧实例引用，避免 start_run
                // 仍通过 instances.first() 取到已失效的旧实例，导致消息转发到死进程。
                let old_ids = session.instances();
                if !old_ids.is_empty() {
                    session.clear_instances();
                    for old_id in &old_ids {
                        let _ = agent_service
                            .stop_and_remove_instance_with_timeout(
                                old_id,
                                std::time::Duration::from_secs(INSTANCE_SHUTDOWN_TIMEOUT_SECS),
                            )
                            .await;
                    }
                }
                session.add_instance(info.id.clone());
                return Ok(info);
            }
        }

        tracing::warn!(
            "[session_manager] session {} disappeared during create_agent, rolling back instance {}",
            session_id,
            info.id
        );
        let _ = agent_service
            .stop_and_remove_instance_with_timeout(
                &info.id,
                std::time::Duration::from_secs(INSTANCE_SHUTDOWN_TIMEOUT_SECS),
            )
            .await;
        self.remove_workspace(session_id).await;
        Err(PluginError::CreateInstanceFailed(
            "session disappeared while creating agent".to_string(),
        ))
    }

    /// Persist the workspace mapping for a session to disk.
    async fn persist_workspace(&self, session_id: &str, work_directory: &str) {
        let mut ws = self.workspaces.write().await;
        ws.insert(
            session_id.to_string(),
            WorkspaceEntry {
                session_id: session_id.to_string(),
                workspace_path: work_directory.to_string(),
                last_used: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            },
        );
        save_workspaces(&ws);
    }

    /// Remove the workspace mapping for a session and persist the change.
    /// Used for rollback when instance creation fails.
    async fn remove_workspace(&self, session_id: &str) {
        let mut ws = self.workspaces.write().await;
        ws.remove(session_id);
        save_workspaces(&ws);
    }

    /// 根据 session_id 获取已记录的 workspace 路径。
    pub async fn workspace_for_session(&self, session_id: &str) -> Option<String> {
        self.workspaces
            .read()
            .await
            .get(session_id)
            .map(|e| e.workspace_path.clone())
    }

    /// 返回最近使用的 N 个 session 及其 workspace，按 last_used 降序排列。
    /// 用于 gatewayd 启动后的预加热。
    pub async fn recent_sessions(&self, limit: usize) -> Vec<(String, String)> {
        let ws = self.workspaces.read().await;
        let mut entries: Vec<_> = ws.values().cloned().collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.last_used));
        entries
            .into_iter()
            .take(limit)
            .map(|e| (e.session_id, e.workspace_path))
            .collect()
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
            .await
            .ok_or(RunError::SessionNotFound)?;

        // 收到用户输入，刷新空闲计时器，防止 session 被回收。
        session.touch();

        let mut instances = session.instances();

        // 如果 session 尚未挂载 agent，且 run 请求携带了 agent_key，自动加载对应 agent。
        if instances.is_empty() {
            if let Some(agent_key) = input.agent_key.as_deref().filter(|s| !s.is_empty()) {
                self.ensure_agent_for_run(session_id, agent_key, &run_id, agent_service)
                    .await?;
                let session = self
                    .get_session(session_id)
                    .await
                    .ok_or(RunError::SessionNotFound)?;
                instances = session.instances();
            }
        }

        if instances.is_empty() {
            return Err(RunError::NoAgent);
        }

        let instance_id = instances.first().cloned().unwrap();
        let message = input
            .messages
            .into_iter()
            .rev()
            .find(|m| matches!(m, Message::User { .. }))
            .and_then(|m| m.content().map(|s| s.to_string()))
            .ok_or(RunError::NoUserMessage)?;

        // 登记当前 run。已有 run 在途时拒绝（AG-UI 同一 thread 同一时间只允许
        // 一个 run），否则回合结束信号（agent.done）会被错误归属到新 run。
        if !session.begin_run(run_id.clone()) {
            return Err(RunError::RunAlreadyActive);
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

        tracing::info!(
            "[session_manager] run={} sending user message to instance={} after {:?}",
            run_id,
            instance_id,
            start.elapsed()
        );

        let send_start = std::time::Instant::now();
        // send_message 仅保证消息已写入 agent 进程，回合在后台异步执行。
        // RUN_FINISHED 不在此发出：它由事件消费者在收到 agent.done（回合真正
        // 结束）时补发，保证排在 TextMessageEnd 等内容事件之后。
        // run_active 由 begin_run/end_run 管理，覆盖回合的整个生命周期，
        // 防止 reap_expired 在长时间 run 期间回收实例。
        if let Err(e) = agent_service
            .send_message(&instance_id, session_id, &message)
            .await
        {
            // 消息未能送达，回合不会开始：立即复位 run 登记。
            session.end_run();
            return Err(RunError::AgentError(e));
        }
        tracing::info!(
            "[session_manager] run={} agent_service.send_message accepted after {:?}",
            run_id,
            send_start.elapsed()
        );

        Ok(run_id)
    }

    /// Subscribe to events for a session.
    pub async fn subscribe(&self, session_id: &str) -> Option<broadcast::Receiver<Event>> {
        self.get_session(session_id).await.map(|s| s.event_tx.subscribe())
    }

    /// Broadcast an event to all subscribers of a session.
    pub async fn broadcast(&self, session_id: &str, event: Event) {
        if let Some(session) = self.get_session(session_id).await {
            let _ = session.event_tx.send(event);
        }
    }

    /// Find the session id that owns the given instance.
    pub async fn session_for_instance(&self, instance_id: &str) -> Option<String> {
        let guard = self.inner.read().await;
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
            let guard = self.inner.read().await;
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
                if let Err(e) = agent_service
                    .stop_and_remove_instance_with_timeout(
                        instance_id,
                        std::time::Duration::from_secs(INSTANCE_SHUTDOWN_TIMEOUT_SECS),
                    )
                    .await
                {
                    tracing::warn!(
                        "[session_manager] failed to stop instance={}: {}",
                        instance_id,
                        e
                    );
                }
            }
            if let Some(session) = self.get_session(&session_id).await {
                session.clear_instances();
            }
        }
    }
}

pub(crate) fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_run_rejects_second_concurrent_run() {
        let session = Session::new("s-1".to_string(), Duration::from_secs(60));
        assert!(session.begin_run("run-1".to_string()));
        // 已有 run 在途时拒绝新 run，且原 run 的登记不被覆盖。
        assert!(!session.begin_run("run-2".to_string()));
        assert_eq!(session.end_run(), Some("run-1".to_string()));
    }

    #[test]
    fn end_run_clears_active_flag_and_returns_none_when_idle() {
        let session = Session::new("s-1".to_string(), Duration::from_secs(60));
        assert!(session.end_run().is_none());
        session.begin_run("run-1".to_string());
        assert!(session.run_active.load(Ordering::SeqCst));
        assert_eq!(session.end_run(), Some("run-1".to_string()));
        assert!(!session.run_active.load(Ordering::SeqCst));
    }

    #[test]
    fn active_run_prevents_expiry() {
        // expired_time 为 0：无 run 的 session 立即过期；
        // 有 run 在途时不得过期（reaper 在长时间 run 期间不得回收实例）。
        let session = Session::new("s-1".to_string(), Duration::from_secs(0));
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(session.is_expired());
        session.begin_run("run-1".to_string());
        assert!(!session.is_expired());
    }
}
