use crate::instance::InstanceStatus;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct CreateInstanceRequest {
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
    #[serde(default)]
    pub force: bool,
}
