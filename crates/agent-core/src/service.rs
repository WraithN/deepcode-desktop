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
use std::time::Duration;

use tokio::sync::Mutex;

use crate::error::{InstanceError, PluginError};
use crate::event_sink::DynEventSink;
use crate::instance::{AgentInstance, InstanceConfig};
use crate::logger::SessionLogger;
use crate::models::{
    CreateInstanceRequest, InstanceInfo, ModelConfig, PluginInfo, UpdateModelConfigRequest,
};
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
/// The first instance for a plugin uses the agent key itself for backwards
/// compatibility. Subsequent instances append an incrementing suffix.
fn unique_instance_id(agent_key: &str, registry: &InstanceRegistry) -> String {
    if registry.get(agent_key).is_none() {
        return agent_key.to_string();
    }

    let mut index = 1u32;
    loop {
        let candidate = format!("{}-{}", agent_key, index);
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
    model_configs: Arc<Mutex<HashMap<String, ModelConfig>>>,
    logger: Arc<SessionLogger>,
    event_sink: DynEventSink,
}

impl AgentService {
    pub fn new(logger: Arc<SessionLogger>, event_sink: DynEventSink) -> Self {
        Self {
            plugins: PluginRegistry::new(),
            instances: Arc::new(Mutex::new(InstanceRegistry::new())),
            model_configs: Arc::new(Mutex::new(HashMap::new())),
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
            .get(&req.agent_key)
            .ok_or(PluginError::NotFound(req.agent_key.clone()))?;

        // When force is not requested, reuse an existing instance that matches
        // the requested agent key, name and work directory. This prevents
        // returning a stale instance that points to a different workspace.
        if !req.force {
            let registry = self.instances.lock().await;
            if let Some((id, existing)) = registry.list().into_iter().find(|(_, i)| {
                i.agent_key() == req.agent_key
                    && i.work_directory() == req.work_directory
                    && i.name() == req.name
            }) {
                return Ok(InstanceInfo {
                    id: id.to_string(),
                    agent_key: req.agent_key.clone(),
                    name: existing.name().to_string(),
                    work_directory: existing.work_directory().to_string(),
                    status: existing.status(),
                    endpoint: existing.endpoint(),
                });
            }
        }

        // 先获取唯一 ID 并释放锁，避免在持有锁期间再次尝试加锁导致死锁。
        let id = {
            let registry = self.instances.lock().await;
            unique_instance_id(&req.agent_key, &*registry)
        };
        let config = InstanceConfig::new(id.clone(), req.name.clone(), req.work_directory.clone());

        let instance = plugin.create_instance(config, self.event_sink.clone())?;
        let info = InstanceInfo {
            id: instance.id().to_string(),
            agent_key: req.agent_key.clone(),
            name: req.name.clone(),
            work_directory: req.work_directory.clone(),
            status: instance.status(),
            endpoint: instance.endpoint(),
        };

        self.instances.lock().await.insert(id, Arc::from(instance));

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

        instance.send_message(conversation_id, message).await
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

    /// Respond to an interaction using the gatewayd conversation id.
    ///
    /// This is the preferred entry point for gatewayd / web clients, which do not
    /// know the agent's internal session id. Plugins that maintain a conversation
    /// -> session mapping (e.g. opencode) will resolve the mapping internally.
    pub async fn respond_to_instance_by_conversation(
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

        instance
            .respond_by_conversation(conversation_id, message)
            .await
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

    /// 停止并从注册表中移除实例，用于 session 过期回收。
    pub async fn stop_and_remove_instance(&self, instance_id: &str) -> Result<(), InstanceError> {
        let instance = {
            let registry = self.instances.lock().await;
            registry
                .get(instance_id)
                .ok_or(InstanceError::NotFound(instance_id.to_string()))?
        };
        instance.stop().await?;
        self.instances.lock().await.remove(instance_id);
        Ok(())
    }

    /// 使用 graceful shutdown 停止并移除实例，等待最多 `timeout` 让进程退出。
    /// 用于 gatewayd 优雅关闭与 session 过期回收，避免僵尸进程。
    pub async fn stop_and_remove_instance_with_timeout(
        &self,
        instance_id: &str,
        timeout: Duration,
    ) -> Result<(), InstanceError> {
        let instance = {
            let registry = self.instances.lock().await;
            registry
                .get(instance_id)
                .ok_or(InstanceError::NotFound(instance_id.to_string()))?
        };
        instance.graceful_shutdown(timeout).await?;
        self.instances.lock().await.remove(instance_id);
        Ok(())
    }

    /// Gracefully stop all running instances and remove them from the registry.
    /// Timeouts are applied per-instance; individual failures are logged but do
    /// not prevent the remaining instances from being stopped.
    pub async fn stop_all_instances_with_timeout(&self, timeout: Duration) {
        let ids: Vec<String> = {
            let registry = self.instances.lock().await;
            registry
                .list()
                .into_iter()
                .map(|(id, _)| id.clone())
                .collect()
        };

        for id in ids {
            if let Err(e) = self
                .stop_and_remove_instance_with_timeout(&id, timeout)
                .await
            {
                log::warn!("failed to gracefully stop instance {}: {}", id, e);
            }
        }
    }

    pub async fn get_instance(&self, instance_id: &str) -> Option<InstanceInfo> {
        let registry = self.instances.lock().await;
        let instance = registry.get(instance_id)?;
        Some(InstanceInfo {
            id: instance.id().to_string(),
            agent_key: instance.agent_key().to_string(),
            name: instance.name().to_string(),
            work_directory: instance.work_directory().to_string(),
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
                agent_key: instance.agent_key().to_string(),
                name: instance.name().to_string(),
                work_directory: instance.work_directory().to_string(),
                status: instance.status(),
                endpoint: instance.endpoint(),
            })
            .collect()
    }

    pub async fn update_model_config(
        &self,
        req: UpdateModelConfigRequest,
    ) -> Result<(), InstanceError> {
        let instance_exists = self.instances.lock().await.get(&req.instance_id).is_some();

        if !instance_exists {
            return Err(InstanceError::NotFound(req.instance_id.clone()));
        }

        let config = ModelConfig {
            model_type: req.model_type,
            model_id: req.model_id,
            model_name: req.model_name,
            url: req.url,
            api_key: req.api_key,
            show_thinking: req.show_thinking,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        };

        self.model_configs
            .lock()
            .await
            .insert(req.instance_id, config);

        Ok(())
    }

    pub async fn get_model_config(&self, instance_id: &str) -> Option<ModelConfig> {
        self.model_configs.lock().await.get(instance_id).cloned()
    }

    pub async fn remove_model_config(&self, instance_id: &str) {
        self.model_configs.lock().await.remove(instance_id);
    }
}
