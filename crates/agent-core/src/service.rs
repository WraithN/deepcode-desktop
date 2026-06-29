// Service orchestration layer shared across the agent runtime.
//
// NOTE: This module is marked with `#![allow(dead_code, unused_imports)]` as a
// workaround for an internal compiler error (ICE) in the custom rustc 1.95.0
// toolchain when dead-code/unused-import lint diagnostics are rendered for
// items in this module. The module contents are legitimate public API surface
// used by `dh-desktop` and `dh-gatewayd`.
#![allow(dead_code, unused_imports)]

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::error::{InstanceError, PluginError};
use crate::event_sink::DynEventSink;
use crate::instance::{AgentInstance, InstanceConfig};
use crate::logger::SessionLogger;
use crate::models::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use crate::plugin::AgentPlugin;

/// Registry of available agent plugins keyed by their unique key.
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn AgentPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn AgentPlugin>) {
        self.plugins.insert(plugin.key().to_string(), plugin);
    }

    pub fn get(&self, key: &str) -> Option<&Box<dyn AgentPlugin>> {
        self.plugins.get(key)
    }

    pub fn list(&self) -> Vec<(&String, &Box<dyn AgentPlugin>)> {
        self.plugins.iter().collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry of running agent instances keyed by instance id.
pub struct InstanceRegistry {
    instances: HashMap<String, Arc<dyn AgentInstance>>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: String, instance: Arc<dyn AgentInstance>) {
        self.instances.insert(id, instance);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn AgentInstance>> {
        self.instances.get(id).cloned()
    }

    pub fn remove(&mut self, id: &str) {
        self.instances.remove(id);
    }

    pub fn list(&self) -> Vec<(&String, &Arc<dyn AgentInstance>)> {
        self.instances.iter().collect()
    }
}

impl Default for InstanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Generates a unique instance id.
///
/// The first instance for a plugin uses the plugin key itself for backwards
/// compatibility. Subsequent instances append an incrementing suffix.
fn unique_instance_id(plugin_key: &str, registry: &InstanceRegistry) -> String {
    if registry.get(plugin_key).is_none() {
        return plugin_key.to_string();
    }

    let mut index = 1u32;
    loop {
        let candidate = format!("{}-{}", plugin_key, index);
        if registry.get(&candidate).is_none() {
            return candidate;
        }
        index += 1;
    }
}

/// High-level service that manages plugins and running instances.
pub struct AgentService {
    plugins: PluginRegistry,
    instances: Arc<Mutex<InstanceRegistry>>,
    logger: Arc<SessionLogger>,
    event_sink: DynEventSink,
}

impl AgentService {
    pub fn new(logger: Arc<SessionLogger>, event_sink: DynEventSink) -> Self {
        Self {
            plugins: PluginRegistry::new(),
            instances: Arc::new(Mutex::new(InstanceRegistry::new())),
            logger,
            event_sink,
        }
    }

    pub fn register_plugin(&mut self, plugin: Box<dyn AgentPlugin>) {
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

        // When force is not requested, reuse an existing instance that matches
        // the requested plugin key.
        if !req.force {
            let registry = self.instances.lock().await;
            if let Some((id, existing)) = registry
                .list()
                .into_iter()
                .find(|(_, i)| i.plugin_key() == req.plugin_key)
            {
                return Ok(InstanceInfo {
                    id: id.to_string(),
                    plugin_key: req.plugin_key.clone(),
                    name: req.name.clone(),
                    workspace: req.workspace.clone(),
                    status: existing.status(),
                    endpoint: existing.endpoint(),
                });
            }
        }

        // 先获取唯一 ID 并释放锁，避免在持有锁期间再次尝试加锁导致死锁。
        let id = {
            let registry = self.instances.lock().await;
            unique_instance_id(&req.plugin_key, &*registry)
        };
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
            workspace: String::new(),
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
                workspace: String::new(),
                status: instance.status(),
                endpoint: instance.endpoint(),
            })
            .collect()
    }
}
