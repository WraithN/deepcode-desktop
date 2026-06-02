use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin not installed: {0}")]
    NotInstalled(String),
    #[error("Failed to create instance: {0}")]
    CreateInstanceFailed(String),
}

#[derive(Error, Debug)]
pub enum InstanceError {
    #[error("Instance not found: {0}")]
    NotFound(String),
    #[error("Instance not running: {0}")]
    NotRunning(String),
    #[error("Failed to send message: {0}")]
    SendFailed(String),
    #[error("Process error: {0}")]
    ProcessError(String),
}
