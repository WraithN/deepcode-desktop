//! Top-level OpenCode adapter.

mod paths;
mod settings;

use crate::adapter::AgentConfigAdapter;
use crate::error::Result;
use crate::types::{ConfigScope, RenderResult, RenderedFile};
use dh_config::UnifiedConfig;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const ADAPTER_KEY: &str = "opencode";
const ADAPTER_NAME: &str = "OpenCode";

/// Adapter that renders the unified config into OpenCode's native file.
pub struct OpencodeAdapter;

impl OpencodeAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OpencodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentConfigAdapter for OpencodeAdapter {
    fn key(&self) -> &'static str {
        ADAPTER_KEY
    }

    fn display_name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn target_paths(&self, scope: &ConfigScope) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        if let Ok(p) = paths::config_path(scope) {
            out.push(p);
        }
        out
    }

    fn render(&self, cfg: &UnifiedConfig, scope: &ConfigScope) -> Result<RenderResult> {
        let mut result = RenderResult::default();
        let config_file = render_config(cfg, scope)?;
        result.push(config_file);
        Ok(result)
    }
}

fn render_config(cfg: &UnifiedConfig, scope: &ConfigScope) -> Result<RenderedFile> {
    let path = paths::config_path(scope)?;
    let existing = read_existing_json(&path)?;
    let body = settings::build(cfg, existing.as_ref())?;
    let bytes = serde_json::to_vec_pretty(&body)?;
    Ok(RenderedFile::new(path, bytes)
        .with_managed_keys(settings::MANAGED_KEYS.iter().map(|s| (*s).to_string()).collect()))
}

fn read_existing_json(path: &std::path::Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|source| crate::error::AdapterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&raw)?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn global_render_only_writes_opencode_config() {
        let cfg = UnifiedConfig::default();
        let adapter = OpencodeAdapter::new();
        let render = adapter.render(&cfg, &ConfigScope::Global).unwrap();
        assert_eq!(render.files.len(), 1);
        assert!(render.files[0].path.ends_with("opencode/opencode.json"));
    }

    #[test]
    fn project_render_writes_opencode_config_in_workspace() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().to_path_buf();
        let cfg = UnifiedConfig::default();
        let adapter = OpencodeAdapter::new();
        let render = adapter
            .render(&cfg, &ConfigScope::Project(workspace.clone()))
            .unwrap();
        assert_eq!(render.files.len(), 1);
        assert!(render.files[0]
            .path
            .ends_with(workspace.join("opencode.json")));
    }
}
