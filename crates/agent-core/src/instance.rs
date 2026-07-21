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
