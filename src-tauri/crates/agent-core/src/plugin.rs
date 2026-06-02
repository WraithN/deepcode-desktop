use crate::error::PluginError;
use crate::instance::{AgentInstance, InstanceConfig};

pub trait AgentPlugin: Send + Sync {
    fn key(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_installed(&self) -> bool;
    fn create_instance(&self, config: InstanceConfig) -> Result<Box<dyn AgentInstance>, PluginError>;
}
