pub mod error;
pub mod event;
pub mod event_sink;
pub mod instance;
pub mod logger;
pub mod mcp;
pub mod plugin;
pub mod process;

pub mod models {
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
    }
}
