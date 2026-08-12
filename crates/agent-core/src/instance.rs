use crate::error::InstanceError;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Stopped,
    Starting,
    Running { pid: u32 },
    Crashed(String),
}

/// Placeholder PID used when the real OS process id is not tracked.
pub const UNKNOWN_PID: u32 = 0;

#[derive(Clone, Debug)]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub work_directory: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
}

impl InstanceConfig {
    pub fn new(id: String, name: String, work_directory: String) -> Self {
        Self {
            id,
            name,
            work_directory,
            session_id: None,
            model: None,
            permission_mode: None,
        }
    }

    /// 设置 agent 内部 session ID，用于 --resume 恢复上下文。
    pub fn with_session_id(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }
}

/// Default polling interval used by `graceful_shutdown` while waiting for the
/// instance to report `InstanceStatus::Stopped`.
const GRACEFUL_SHUTDOWN_POLL_INTERVAL_MS: u64 = 100;

pub trait AgentInstance: Send + Sync {
    fn id(&self) -> &str;
    fn status(&self) -> InstanceStatus;
    fn agent_key(&self) -> &'static str;
    fn name(&self) -> &str;
    fn work_directory(&self) -> &str;

    /// Optional endpoint URL for this instance (e.g. opencode serve URL).
    fn endpoint(&self) -> Option<String> {
        None
    }

    /// 返回 agent 内部当前活跃的 session ID（如 claude 的 session_id）。
    /// 用于 gatewayd 在 reap 前持久化，以便重建实例时通过 --resume 恢复上下文。
    /// 默认返回 None；支持 resume 的 plugin（如 claude）应覆盖此方法。
    fn active_session_id(&self) -> Option<String> {
        None
    }

    /// 设置 SSE 看门狗无事件超时阈值（秒）。
    /// 由 `AgentService::update_model_config` 在收到模型设置更新时调用，
    /// 使看门狗阈值可通过模型设置接口动态调整。
    /// 默认空实现；支持看门狗的 plugin（如 opencode）应覆盖此方法。
    fn set_watchdog_timeout(&self, _secs: u64) {}

    /// 活性探针：看门狗在静默超窗后调用，区分"agent 忙但活着"（如 LLM 长
    /// 生成期间无 SSE 事件）与"进程真卡死"。返回 true 表示存活。
    ///
    /// 默认实现为弱检查（`status() == Running` 即视为存活）：进程已崩溃但
    /// 插件未及时更新 status 时会偏乐观，因此有 HTTP/IPC 端点或进程句柄的
    /// 插件（如 opencode）应覆盖为真实探活。
    fn liveness_probe(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        Box::pin(async move { matches!(self.status(), InstanceStatus::Running { .. }) })
    }

    fn send_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;

    /// Send a response to an interaction (question/permission/todo).
    fn respond(
        &self,
        session_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;

    /// Send a response to an interaction using the conversation id.
    ///
    /// The default implementation delegates to [`respond`] with the conversation id
    /// as the session id, which is sufficient for plugins that do not maintain a
    /// separate internal session mapping. Plugins like opencode that map conversation
    /// ids to internal agent session ids should override this method.
    fn respond_by_conversation(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let conversation_id = conversation_id.to_string();
        let message = message.to_string();
        Box::pin(async move { self.respond(&conversation_id, &message).await })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;

    /// Gracefully stop the instance, waiting up to `timeout` for the process to
    /// report `Stopped`.  The default implementation calls `stop()` and polls the
    /// instance status until it stops or the timeout expires.
    fn graceful_shutdown(
        &self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            self.stop().await?;
            let deadline = std::time::Instant::now() + timeout;
            while std::time::Instant::now() < deadline {
                if matches!(self.status(), InstanceStatus::Stopped) {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(GRACEFUL_SHUTDOWN_POLL_INTERVAL_MS)).await;
            }
            Err(InstanceError::ProcessError(
                "instance did not stop within graceful shutdown timeout".to_string(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MODEL: &str = "sonnet";
    const TEST_PERMISSION_MODE: &str = "bypassPermissions";

    #[test]
    fn test_instance_status_serde() {
        let s = serde_json::to_string(&InstanceStatus::Stopped).unwrap();
        assert_eq!(s, r#""stopped""#);
        let s = serde_json::to_string(&InstanceStatus::Running { pid: 1234 }).unwrap();
        assert_eq!(s, r#"{"running":{"pid":1234}}"#);
        let s = serde_json::to_string(&InstanceStatus::Crashed("oops".into())).unwrap();
        assert_eq!(s, r#"{"crashed":"oops"}"#);
    }

    #[test]
    fn test_instance_config() {
        let cfg = InstanceConfig {
            id: "i-1".into(),
            name: "test".into(),
            work_directory: "/tmp".into(),
            session_id: Some("s-1".into()),
            model: Some(TEST_MODEL.into()),
            permission_mode: Some(TEST_PERMISSION_MODE.into()),
        };
        assert_eq!(cfg.id, "i-1");
        assert_eq!(cfg.model.as_deref(), Some(TEST_MODEL));
        assert_eq!(cfg.permission_mode.as_deref(), Some(TEST_PERMISSION_MODE));
    }
}
