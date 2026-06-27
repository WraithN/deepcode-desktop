//! Strongly-typed schema for the unified dh configuration.
//!
//! This is the in-memory representation of the user-facing TOML files
//! (`~/.config/dh/config.toml` and `<repo>/.dh/config.toml`). Adapters consume
//! [`UnifiedConfig`] to render agent-native configuration files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Top-level configuration document.
///
/// All fields are optional at the file level; merge logic fills in defaults
/// and combines layers (global ⊕ profile ⊕ project).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnifiedConfig {
    /// Default profile name (only meaningful at the global scope).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,

    /// LLM provider definitions keyed by provider id (e.g. `deepseek`).
    pub providers: BTreeMap<String, ProviderConfig>,

    /// Named model entries. Always contains a `default` key after validation.
    pub models: BTreeMap<String, ModelConfig>,

    /// MCP server definitions. Order is preserved.
    pub mcp: Vec<McpServerConfig>,

    /// Skill configuration (search paths and enabled list).
    pub skills: SkillsConfig,

    /// Engineering rules (system-prompt fragments).
    pub rules: RulesConfig,
}

/// Provider definition (base URL + credentials).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Free-form metadata passed through to adapter-specific fields.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

/// Named model entry referenced by skills or adapters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    /// Provider key referencing [`UnifiedConfig::providers`].
    pub provider: String,
    /// Model name as understood by the provider (e.g. `deepseek-coder`).
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// One MCP server entry.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Restrict to specific agent adapters by key (`opencode`, `claudecode`).
    /// Empty list means "all adapters".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Skills configuration block.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkillsConfig {
    /// Directories scanned for `*.md` skill files.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub search_paths: Vec<PathBuf>,
    /// Skill names that should be exposed to agents.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enabled: Vec<String>,
}

/// Rules configuration block.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RulesConfig {
    /// Markdown files concatenated into the system prompt fragment.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<PathBuf>,
}

impl UnifiedConfig {
    /// Convenience: returns the default model entry if defined.
    pub fn default_model(&self) -> Option<&ModelConfig> {
        self.models.get("default")
    }

    /// Returns true when the configuration has no user-provided content.
    pub fn is_empty(&self) -> bool {
        self.default_profile.is_none()
            && self.providers.is_empty()
            && self.models.is_empty()
            && self.mcp.is_empty()
            && self.skills.search_paths.is_empty()
            && self.skills.enabled.is_empty()
            && self.rules.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let cfg = UnifiedConfig::default();
        assert!(cfg.is_empty());
        assert!(cfg.default_model().is_none());
    }

    #[test]
    fn parses_minimal_toml() {
        let src = r#"
default_profile = "work"

[providers.deepseek]
api_key = "sk-xxx"

[models.default]
provider = "deepseek"
name = "deepseek-coder"

[[mcp]]
name = "filesystem"
command = "npx"
args = ["@modelcontextprotocol/server-filesystem"]

[skills]
enabled = ["code-review"]
"#;
        let cfg: UnifiedConfig = toml::from_str(src).unwrap();
        assert_eq!(cfg.default_profile.as_deref(), Some("work"));
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.default_model().unwrap().name, "deepseek-coder");
        assert_eq!(cfg.mcp.len(), 1);
        assert!(cfg.mcp[0].enabled);
        assert_eq!(cfg.skills.enabled, vec!["code-review"]);
    }

    #[test]
    fn rejects_unknown_fields() {
        let src = r#"
nonsense_field = 1
"#;
        let result: std::result::Result<UnifiedConfig, _> = toml::from_str(src);
        assert!(result.is_err());
    }
}
