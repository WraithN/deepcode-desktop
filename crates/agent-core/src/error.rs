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
    #[error("MCP error: {0}")]
    #[deprecated(
        since = "0.2.0",
        note = "MCP stack is being removed; use ProcessError instead"
    )]
    McpError(String),
    /// 当前 run 被交互式工具（如 question）暂停，等待用户响应。
    /// 这不是真正的错误，插件已在 SSE relay loop 中 emit agent.question/agent.done，
    /// 需要上层直接结束本 run，避免触发重试或错误广播。
    #[error("Interaction cancelled: {0}")]
    InteractionCancelled(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_error_display() {
        assert_eq!(
            format!("{}", PluginError::NotFound("opencode".into())),
            "Plugin not found: opencode"
        );
        assert_eq!(
            format!("{}", PluginError::NotInstalled("opencode".into())),
            "Plugin not installed: opencode"
        );
    }

    #[test]
    fn test_instance_error_display() {
        assert_eq!(
            format!("{}", InstanceError::NotFound("abc".into())),
            "Instance not found: abc"
        );
        assert_eq!(
            format!("{}", InstanceError::ProcessError("killed".into())),
            "Process error: killed"
        );
    }
}
