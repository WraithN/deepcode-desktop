use crate::error::InstanceError;
use async_trait::async_trait;
use serde::Serialize;

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

#[async_trait]
pub trait AgentInstance: Send + Sync {
    fn id(&self) -> &str;
    fn status(&self) -> InstanceStatus;
    async fn send_message(&self, message: &str) -> Result<(), InstanceError>;
    async fn stop(&self) -> Result<(), InstanceError>;
}
