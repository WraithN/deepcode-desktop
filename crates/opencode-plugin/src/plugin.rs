use agent_core::error::PluginError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig};
use agent_core::logger::SessionLogger;
use agent_core::plugin::AgentPlugin;
use std::sync::Arc;

pub struct OpencodePlugin {
    logger: Arc<SessionLogger>,
}

impl OpencodePlugin {
    pub fn new(logger: Arc<SessionLogger>) -> Self {
        Self { logger }
    }
}

impl AgentPlugin for OpencodePlugin {
    fn key(&self) -> &'static str {
        "opencode"
    }

    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn is_installed(&self) -> bool {
        std::process::Command::new("opencode")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn create_instance(
        &self,
        config: InstanceConfig,
        event_sink: DynEventSink,
    ) -> Result<Box<dyn AgentInstance>, PluginError> {
        if !self.is_installed() {
            return Err(PluginError::NotInstalled("opencode".to_string()));
        }
        Ok(Box::new(crate::instance::OpencodeInstance::new(
            config,
            event_sink,
            self.logger.clone(),
        )))
    }
}
