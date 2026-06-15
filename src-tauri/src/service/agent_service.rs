#![allow(dead_code)]

use crate::models::agent::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use agent_core::error::{InstanceError, PluginError};
use agent_core::event_sink::DynEventSink;
use agent_core::instance::InstanceConfig;
use agent_core::logger::SessionLogger;
use std::sync::Arc;

pub struct AgentService {
    plugins: super::plugin_registry::PluginRegistry,
    instances: Arc<tokio::sync::Mutex<super::instance_registry::InstanceRegistry>>,
    logger: Arc<SessionLogger>,
    event_sink: DynEventSink,
}

impl AgentService {
    pub fn new(logger: Arc<SessionLogger>, event_sink: DynEventSink) -> Self {
        let plugins = super::plugin_registry::PluginRegistry::new();
        Self {
            plugins,
            instances: Arc::new(tokio::sync::Mutex::new(super::instance_registry::InstanceRegistry::new())),
            logger,
            event_sink,
        }
    }

    pub fn register_plugin(&mut self, plugin: Box<dyn agent_core::plugin::AgentPlugin>) {
        self.plugins.register(plugin);
    }

    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .list()
            .into_iter()
            .map(|(key, p)| PluginInfo {
                key: key.clone(),
                name: p.name().to_string(),
                installed: p.is_installed(),
            })
            .collect()
    }

    pub async fn create_instance(
        &self,
        req: CreateInstanceRequest,
    ) -> Result<InstanceInfo, PluginError> {
        let plugin = self
            .plugins
            .get(&req.plugin_key)
            .ok_or(PluginError::NotFound(req.plugin_key.clone()))?;

        let id = format!("{}-{}", req.plugin_key, uuid::Uuid::new_v4());
        let config = InstanceConfig::new(id.clone(), req.name.clone(), req.workspace.clone());

        let instance = plugin.create_instance(config, self.event_sink.clone())?;
        let info = InstanceInfo {
            id: instance.id().to_string(),
            plugin_key: req.plugin_key.clone(),
            name: req.name.clone(),
            workspace: req.workspace.clone(),
            status: instance.status(),
            endpoint: instance.endpoint(),
        };

        self.instances
            .lock()
            .await
            .insert(id, Arc::from(instance));

        Ok(info)
    }

    pub async fn send_message(
        &self,
        instance_id: &str,
        conversation_id: &str,
        message: &str,
    ) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        let message = message.to_string();
        let conversation_id = conversation_id.to_string();
        tokio::spawn(async move {
            let _ = instance.send_message(&conversation_id, &message).await;
        });

        Ok(())
    }

    pub async fn respond_to_instance(
        &self,
        instance_id: &str,
        session_id: &str,
        message: &str,
    ) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        instance.respond(session_id, message).await
    }

    pub async fn stop_instance(&self, instance_id: &str) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        instance.stop().await
    }

    pub async fn get_instance(&self, instance_id: &str) -> Option<InstanceInfo> {
        let registry = self.instances.lock().await;
        let instance = registry.get(instance_id)?;
        Some(InstanceInfo {
            id: instance.id().to_string(),
            plugin_key: instance.plugin_key().to_string(),
            name: instance.id().to_string(),
            workspace: "".to_string(),
            status: instance.status(),
            endpoint: instance.endpoint(),
        })
    }

    pub async fn list_instances(&self) -> Vec<InstanceInfo> {
        let registry = self.instances.lock().await;
        registry
            .list()
            .into_iter()
            .map(|(id, instance)| InstanceInfo {
                id: id.clone(),
                plugin_key: instance.plugin_key().to_string(),
                name: instance.id().to_string(),
                workspace: "".to_string(),
                status: instance.status(),
                endpoint: instance.endpoint(),
            })
            .collect()
    }
}
