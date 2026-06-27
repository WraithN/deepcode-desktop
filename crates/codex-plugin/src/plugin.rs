use agent_core::error::PluginError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig};
use agent_core::logger::SessionLogger;
use agent_core::plugin::{is_command_installed, AgentPlugin};
use std::sync::Arc;

use crate::constants::{PLUGIN_KEY, PLUGIN_NAME, PROGRAM_CODEX, VERSION_FLAG};

pub struct CodexPlugin {
    logger: Arc<SessionLogger>,
}

impl CodexPlugin {
    pub fn new(logger: Arc<SessionLogger>) -> Self {
        Self { logger }
    }
}

impl AgentPlugin for CodexPlugin {
    fn key(&self) -> &'static str {
        PLUGIN_KEY
    }

    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn is_installed(&self) -> bool {
        is_command_installed(PROGRAM_CODEX, VERSION_FLAG)
    }

    fn create_instance(
        &self,
        config: InstanceConfig,
        event_sink: DynEventSink,
    ) -> Result<Box<dyn AgentInstance>, PluginError> {
        if !self.is_installed() {
            return Err(PluginError::NotInstalled(PROGRAM_CODEX.to_string()));
        }
        let instance = crate::instance::CodexInstance::new(config, event_sink, self.logger.clone());
        Ok(Box::new(instance))
    }
}
