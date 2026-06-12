use crate::error::PluginError;
use crate::event_sink::DynEventSink;
use crate::instance::{AgentInstance, InstanceConfig};

pub trait AgentPlugin: Send + Sync {
    fn key(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_installed(&self) -> bool;

    /// Create a new agent instance.
    ///
    /// The `event_sink` is provided by the runtime (Desktop WebSocket or
    /// gatewayd HTTP channel) and must be stored by the instance so that
    /// all events can be emitted through it.
    fn create_instance(
        &self,
        config: InstanceConfig,
        event_sink: DynEventSink,
    ) -> Result<Box<dyn AgentInstance>, PluginError>;
}
