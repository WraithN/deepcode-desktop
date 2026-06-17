use agent_core::error::PluginError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig};
use agent_core::logger::SessionLogger;
use agent_core::plugin::{is_command_installed, AgentPlugin};
use std::sync::Arc;

const PLUGIN_KEY: &str = "opencode";
const PLUGIN_NAME: &str = "OpenCode";
const PROGRAM: &str = "opencode";
const VERSION_FLAG: &str = "--version";

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
        PLUGIN_KEY
    }

    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn is_installed(&self) -> bool {
        is_command_installed(PROGRAM, VERSION_FLAG)
    }

    fn create_instance(
        &self,
        config: InstanceConfig,
        event_sink: DynEventSink,
    ) -> Result<Box<dyn AgentInstance>, PluginError> {
        if !self.is_installed() {
            return Err(PluginError::NotInstalled(PROGRAM.to_string()));
        }
        Ok(Box::new(crate::instance::OpencodeInstance::new(
            config,
            event_sink,
            self.logger.clone(),
        )))
    }
}
