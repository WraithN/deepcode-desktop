use crate::instance::InstanceStatus;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct PluginInfo {
    pub key: String,
    pub name: String,
    pub installed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstanceInfo {
    pub id: String,
    pub agent_key: String,
    pub name: String,
    pub work_directory: String,
    pub status: InstanceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateInstanceRequest {
    pub agent_key: String,
    pub name: String,
    pub work_directory: String,
    #[serde(default)]
    pub force: bool,
    /// Agent 内部的 session ID（如 claude 的 session_id），用于 instance 被
    /// reap 后重建时通过 --resume 恢复上下文。由 gatewayd 从持久化存储中
    /// 查询并填充，调用方通常不需要显式传入。
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default)]
    pub model_type: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub show_thinking: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateModelConfigRequest {
    pub instance_id: String,
    #[serde(default)]
    pub model_type: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub show_thinking: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}
