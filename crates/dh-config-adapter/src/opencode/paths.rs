//! Path helpers for the OpenCode adapter.
//!
//! OpenCode 支持两个作用域（见 opencode 文档 config 优先级）：
//! * Global: `~/.config/opencode/opencode.json`
//! * Project: `<workspace>/opencode.json`

use crate::constants::{OPENCODE_CONFIG_FILE, OPENCODE_USER_DIRNAME};
use crate::error::{AdapterError, Result};
use crate::types::ConfigScope;
use std::path::PathBuf;

/// 解析指定作用域的 `opencode.json` 路径。
pub fn config_path(scope: &ConfigScope) -> Result<PathBuf> {
    match scope {
        ConfigScope::Global => global_config_path(),
        ConfigScope::Project(workspace) => Ok(workspace.join(OPENCODE_CONFIG_FILE)),
    }
}

/// 全局配置路径：`~/.config/opencode/opencode.json`。
fn global_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        AdapterError::Unsupported("could not determine config directory".into())
    })?;
    Ok(config_dir.join(OPENCODE_USER_DIRNAME).join(OPENCODE_CONFIG_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_path() {
        let ws = PathBuf::from("/tmp/repo");
        let scope = ConfigScope::Project(ws);
        assert!(config_path(&scope)
            .unwrap()
            .ends_with("opencode.json"));
    }
}
