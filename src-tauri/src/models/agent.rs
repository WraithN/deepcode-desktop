use agent_core::instance::InstanceStatus;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct PluginInfo {
    pub key: String,
    pub name: String,
    pub installed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstanceInfo {
    pub id: String,
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
    pub status: InstanceStatus,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct CreateInstanceRequest {
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
}
