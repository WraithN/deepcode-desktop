use crate::error::InstanceError;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Stopped,
    Starting,
    Running { pid: u32 },
    Crashed(String),
}

#[derive(Clone, Debug)]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub session_id: Option<String>,
}

pub trait AgentInstance: Send + Sync {
    fn id(&self) -> &str;
    fn status(&self) -> InstanceStatus;
    fn plugin_key(&self) -> &'static str;

    fn send_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;
}

#[cfg(test)]
mod tests {
    use super::*;

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
            workspace: "/tmp".into(),
            session_id: Some("s-1".into()),
        };
        assert_eq!(cfg.id, "i-1");
        assert_eq!(cfg.session_id.as_deref(), Some("s-1"));
    }
}
