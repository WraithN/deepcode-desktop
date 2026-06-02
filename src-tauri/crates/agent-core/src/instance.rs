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
    fn send_message(
        &self,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;
    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;
}
