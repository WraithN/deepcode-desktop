#![allow(dead_code)]

use crate::models::agent::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use agent_core::error::{InstanceError, PluginError};
use agent_core::instance::InstanceConfig;
use agent_core::logger::SessionLogger;
use std::sync::Arc;

pub struct AgentService {
    plugins: super::plugin_registry::PluginRegistry,
    instances: Arc<tokio::sync::Mutex<super::instance_registry::InstanceRegistry>>,
    logger: Arc<SessionLogger>,
}

impl AgentService {
    pub fn new(logger: Arc<SessionLogger>) -> Self {
        let plugins = super::plugin_registry::PluginRegistry::new();
        Self {
            plugins,
            instances: Arc::new(tokio::sync::Mutex::new(super::instance_registry::InstanceRegistry::new())),
            logger,
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
        let config = InstanceConfig {
            id: id.clone(),
            name: req.name.clone(),
            workspace: req.workspace.clone(),
            session_id: None,
        };

        let instance = plugin.create_instance(config)?;
        let info = InstanceInfo {
            id: instance.id().to_string(),
            plugin_key: req.plugin_key.clone(),
            name: req.name.clone(),
            workspace: req.workspace.clone(),
            status: instance.status(),
        };

        self.instances
            .lock()
            .await
            .insert(id, Arc::new(tokio::sync::Mutex::new(instance)));

        Ok(info)
    }

    pub async fn send_message(
        &self,
        instance_id: &str,
        message: &str,
    ) -> Result<(), InstanceError> {
        let instance_arc = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        let instance = instance_arc.lock().await;
        instance.send_message(message).await
    }

    pub async fn stop_instance(&self, instance_id: &str) -> Result<(), InstanceError> {
        let instance_arc = self
            .instances
            .lock()
            .await
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        let instance = instance_arc.lock().await;
        instance.stop().await
    }

    pub async fn get_instance(&self, instance_id: &str) -> Option<InstanceInfo> {
        let registry = self.instances.lock().await;
        let instance = registry.get(instance_id)?;
        let guard = instance.lock().await;
        Some(InstanceInfo {
            id: guard.id().to_string(),
            plugin_key: "unknown".to_string(),
            name: guard.id().to_string(),
            workspace: "".to_string(),
            status: guard.status(),
        })
    }

    pub async fn list_instances(&self) -> Vec<InstanceInfo> {
        let registry = self.instances.lock().await;
        let mut result = Vec::new();
        for (id, instance) in registry.list() {
            let guard = instance.lock().await;
            result.push(InstanceInfo {
                id: id.clone(),
                plugin_key: "unknown".to_string(),
                name: guard.id().to_string(),
                workspace: "".to_string(),
                status: guard.status(),
            });
        }
        result
    }
}
