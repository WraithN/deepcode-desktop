//! Top-level Claude Code adapter.

mod memory;
mod paths;
mod settings;

use crate::adapter::AgentConfigAdapter;
use crate::error::Result;
use crate::types::{ConfigScope, RenderResult, RenderedFile};
use dh_config::UnifiedConfig;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const ADAPTER_KEY: &str = "claudecode";
const ADAPTER_NAME: &str = "Claude Code";

/// Adapter that renders the unified config into Claude Code's native files.
pub struct ClaudecodeAdapter;

impl ClaudecodeAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ClaudecodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentConfigAdapter for ClaudecodeAdapter {
    fn key(&self) -> &'static str {
        ADAPTER_KEY
    }

    fn display_name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn target_paths(&self, scope: &ConfigScope) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        if let Ok(p) = paths::settings_path(scope) {
            out.push(p);
        }
        if let ConfigScope::Project(_) = scope {
            if let Ok(p) = paths::memory_path(scope) {
                out.push(p);
            }
        }
        out
    }

    fn render(&self, cfg: &UnifiedConfig, scope: &ConfigScope) -> Result<RenderResult> {
        let mut result = RenderResult::default();

        let settings_file = render_settings(cfg, scope)?;
        result.push(settings_file);

        if let ConfigScope::Project(workspace) = scope {
            if !cfg.rules.files.is_empty() {
                let memory_file = render_memory(workspace, cfg)?;
                result.push(memory_file);
            }
        }

        for file in render_skill_commands(cfg, scope)? {
            result.push(file);
        }

        Ok(result)
    }
}

// ───── helpers ─────

fn render_settings(cfg: &UnifiedConfig, scope: &ConfigScope) -> Result<RenderedFile> {
    let path = paths::settings_path(scope)?;
    let existing = read_existing_json(&path)?;
    let body = settings::build(cfg, existing.as_ref())?;
    let bytes = serde_json::to_vec_pretty(&body)?;
    Ok(RenderedFile::new(path, bytes)
        .with_managed_keys(settings::MANAGED_KEYS.iter().map(|s| (*s).to_string()).collect()))
}

fn render_memory(
    workspace: &std::path::Path,
    cfg: &UnifiedConfig,
) -> Result<RenderedFile> {
    let body = memory::render_memory(workspace, cfg)?;
    let path = workspace.join(crate::constants::CLAUDE_MEMORY_FILE);
    Ok(RenderedFile::new(path, body.into_bytes()))
}

fn render_skill_commands(
    cfg: &UnifiedConfig,
    scope: &ConfigScope,
) -> Result<Vec<RenderedFile>> {
    if cfg.skills.enabled.is_empty() {
        return Ok(Vec::new());
    }
    let dir = paths::commands_dir(scope)?;
    let mut files = Vec::with_capacity(cfg.skills.enabled.len());
    for skill in &cfg.skills.enabled {
        let (relative, body) = memory::render_skill_command(skill);
        let path = dir.join(relative);
        files.push(RenderedFile::new(path, body.into_bytes()));
    }
    Ok(files)
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
    use dh_config::{ModelConfig, ProviderConfig};
    use tempfile::TempDir;

    fn cfg_with_provider() -> UnifiedConfig {
        let mut cfg = UnifiedConfig::default();
        cfg.providers.insert(
            "anthropic".into(),
            ProviderConfig {
                api_key: Some("sk-1".into()),
                base_url: Some("https://api.deepseek.com/v1".into()),
                ..Default::default()
            },
        );
        cfg.models.insert(
            "default".into(),
            ModelConfig {
                provider: "anthropic".into(),
                name: "deepseek-chat".into(),
                ..Default::default()
            },
        );
        cfg
    }

    #[test]
    fn project_render_includes_settings_and_memory() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().to_path_buf();
        let mut cfg = cfg_with_provider();
        let rule = workspace.join("RULES.md");
        fs::write(&rule, "Be careful.\n").unwrap();
        cfg.rules.files.push(rule);

        let adapter = ClaudecodeAdapter::new();
        let render = adapter
            .render(&cfg, &ConfigScope::Project(workspace.clone()))
            .unwrap();

        // settings.json + CLAUDE.md
        assert_eq!(render.files.len(), 2);
        let settings_file = render
            .files
            .iter()
            .find(|f| f.path.ends_with("settings.json"))
            .unwrap();
        let parsed: Value = serde_json::from_slice(&settings_file.content).unwrap();
        assert_eq!(parsed["env"]["ANTHROPIC_MODEL"], "deepseek-chat");

        let memory_file = render
            .files
            .iter()
            .find(|f| f.path.ends_with("CLAUDE.md"))
            .unwrap();
        let body = String::from_utf8(memory_file.content.clone()).unwrap();
        assert!(body.contains("Be careful"));
    }

    #[test]
    fn global_render_only_writes_settings() {
        let tmp = TempDir::new().unwrap();
        std::env::set_var("HOME", tmp.path());
        let cfg = cfg_with_provider();
        let adapter = ClaudecodeAdapter::new();
        let render = adapter.render(&cfg, &ConfigScope::Global).unwrap();
        assert_eq!(render.files.len(), 1);
        assert!(render.files[0].path.ends_with(".claude/settings.json"));
    }

    #[test]
    fn skill_files_emitted_for_enabled_list() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().to_path_buf();
        let mut cfg = cfg_with_provider();
        cfg.skills.enabled.push("code-review".into());
        cfg.skills.enabled.push("refactor".into());
        let adapter = ClaudecodeAdapter::new();
        let render = adapter
            .render(&cfg, &ConfigScope::Project(workspace.clone()))
            .unwrap();
        let names: Vec<String> = render
            .files
            .iter()
            .filter_map(|f| {
                f.path
                    .file_name()
                    .and_then(|s| s.to_str().map(String::from))
            })
            .collect();
        assert!(names.iter().any(|n| n == "code-review.md"));
        assert!(names.iter().any(|n| n == "refactor.md"));
    }
}
