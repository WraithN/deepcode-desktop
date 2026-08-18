//! Strongly-typed schema for the unified dh configuration.
//!
//! This is the in-memory representation of the user-facing TOML files
//! (`~/.config/dh/config.toml` and `<repo>/.dh/config.toml`). Adapters consume
//! [`UnifiedConfig`] to render agent-native configuration files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::constants::{DEFAULT_REPORT_INTERVAL_SECS, DEFAULT_REQUEST_TIMEOUT_SECS};

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

    /// Platform integration (remote config fetch + session/agent/monitoring reporting).
    #[serde(default)]
    pub platform: PlatformConfig,
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

/// MCP server 传输类型（dh-config 配置层）。
///
/// 与 gatewayd 运行时 `mcp_aggregator::TransportKind` 语义一致但类型独立，
/// 命名为 `TransportKindCfg` 以避免跨 crate 命名冲突。
///
/// - `Stdio`：通过子进程 stdio 通信（command/args/env）。
/// - `Http`：通过 MCP Streamable HTTP 协议访问远程 server（url）。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum TransportKindCfg {
    #[default]
    Stdio,
    Http,
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
    /// 传输类型；缺省为 `Stdio`（向后兼容仅含 command/args/env 的旧配置）。
    #[serde(default)]
    pub transport: TransportKindCfg,
    /// Http transport 用的 URL；Stdio 行可为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
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

/// Platform integration configuration.
///
/// Configured via the `[platform]` section in `config.toml`.
/// When `enabled` is true and `url` is set, the desktop app will:
/// - Fetch remote platform config at startup
/// - Report session info, agent status, and monitoring data to the platform
/// - Report Agent Runtime status to the DH Backend
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlatformConfig {
    /// Platform base URL, e.g. `https://platform.deepharness.com`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Bearer token used for authentication with the platform API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Master switch for platform integration. Default: `false`.
    pub enabled: bool,

    /// Interval between periodic batch reports, in seconds.
    /// Falls back to [`DEFAULT_REPORT_INTERVAL_SECS`] when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_interval_secs: Option<u64>,

    /// Maximum interval between periodic batch reports, in seconds.
    ///
    /// When set and greater than [`report_interval_secs`], the reporter will
    /// pick a random interval within `[report_interval_secs, report_interval_max_secs]`
    /// for each cycle. This helps prevent thundering herd when many runtimes
    /// are started simultaneously.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_interval_max_secs: Option<u64>,

    /// Per-request HTTP timeout for platform API calls, in seconds.
    /// Falls back to [`DEFAULT_REQUEST_TIMEOUT_SECS`] when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_timeout_secs: Option<u64>,

    /// When true, session message content is hashed before reporting
    /// to avoid sending raw user/agent text to the platform.
    pub sanitize: bool,

    /// Runtime ID used for Agent Runtime status reporting
    /// (`POST /api/v1/agent-runtimes/{runtimeId}/status`).
    ///
    /// If unset, the desktop application will auto-generate a UUID and
    /// persist it in the application data directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<String>,

    /// Workspace identifier for the runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,

    /// User identifier for the runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

impl PlatformConfig {
    /// Returns the effective report interval, falling back to the default.
    pub fn report_interval(&self) -> u64 {
        self.report_interval_secs
            .unwrap_or(DEFAULT_REPORT_INTERVAL_SECS)
    }

    /// Returns the effective maximum report interval.
    ///
    /// Returns `None` when randomization is disabled (no max set or max is
    /// not greater than the base interval).
    pub fn report_interval_max(&self) -> Option<u64> {
        let max = self.report_interval_max_secs?;
        let base = self.report_interval();
        if max > base {
            Some(max)
        } else {
            None
        }
    }

    /// Returns the effective request timeout, falling back to the default.
    pub fn request_timeout(&self) -> u64 {
        self.request_timeout_secs
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_SECS)
    }

    /// Returns true when the platform integration is ready to operate:
    /// enabled, with a URL configured.
    pub fn is_active(&self) -> bool {
        self.enabled && self.url.as_ref().is_some_and(|u| !u.trim().is_empty())
    }

    /// Returns true when no platform-specific content has been configured.
    pub fn is_empty(&self) -> bool {
        self.url.is_none()
            && self.api_key.is_none()
            && !self.enabled
            && self.report_interval_secs.is_none()
            && self.report_interval_max_secs.is_none()
            && self.request_timeout_secs.is_none()
            && !self.sanitize
            && self.runtime_id.is_none()
            && self.workspace_id.is_none()
            && self.user_id.is_none()
    }

    /// Returns true when the runtime status endpoint can be used.
    ///
    /// Requires a runtime id and a workspace id. The tenant id is derived
    /// from the workspace id on the platform side.
    pub fn is_runtime_reporting_active(&self) -> bool {
        self.is_active()
            && self
                .runtime_id
                .as_ref()
                .is_some_and(|id| !id.trim().is_empty())
            && self
                .workspace_id
                .as_ref()
                .is_some_and(|id| !id.trim().is_empty())
    }
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
            && self.platform.is_empty()
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
    fn parses_http_mcp_entry() {
        let src = r#"
[[mcp]]
name = "gatewayd"
transport = "Http"
url = "http://127.0.0.1:2346/mcp"
"#;
        let cfg: UnifiedConfig = toml::from_str(src).unwrap();
        assert_eq!(cfg.mcp.len(), 1);
        assert_eq!(cfg.mcp[0].transport, TransportKindCfg::Http);
        assert_eq!(
            cfg.mcp[0].url.as_deref(),
            Some("http://127.0.0.1:2346/mcp")
        );
    }

    #[test]
    fn mcp_transport_defaults_to_stdio() {
        let src = r#"
[[mcp]]
name = "filesystem"
command = "npx"
"#;
        let cfg: UnifiedConfig = toml::from_str(src).unwrap();
        assert_eq!(cfg.mcp[0].transport, TransportKindCfg::Stdio);
        assert!(cfg.mcp[0].url.is_none());
    }

    #[test]
    fn rejects_unknown_fields() {
        let src = r#"
nonsense_field = 1
"#;
        let result: std::result::Result<UnifiedConfig, _> = toml::from_str(src);
        assert!(result.is_err());
    }

    #[test]
    fn parses_platform_section() {
        let src = r#"
[platform]
url = "https://platform.deepharness.com"
api_key = "secret-token"
enabled = true
report_interval_secs = 60
sanitize = true
"#;
        let cfg: UnifiedConfig = toml::from_str(src).unwrap();
        assert!(cfg.platform.is_active());
        assert_eq!(
            cfg.platform.url.as_deref(),
            Some("https://platform.deepharness.com")
        );
        assert_eq!(cfg.platform.api_key.as_deref(), Some("secret-token"));
        assert_eq!(cfg.platform.report_interval(), 60);
        assert!(cfg.platform.sanitize);
        assert!(!cfg.platform.is_empty());
    }

    #[test]
    fn platform_defaults_to_inactive() {
        let cfg = UnifiedConfig::default();
        assert!(!cfg.platform.is_active());
        assert!(cfg.platform.is_empty());
        assert_eq!(cfg.platform.report_interval(), DEFAULT_REPORT_INTERVAL_SECS);
        assert_eq!(cfg.platform.request_timeout(), DEFAULT_REQUEST_TIMEOUT_SECS);
    }

    #[test]
    fn parses_runtime_reporting_section() {
        let src = r#"
[platform]
url = "https://platform.deepharness.com"
api_key = "secret-token"
enabled = true
runtime_id = "desktop-01"
workspace_id = "w1"
user_id = "u1"
"#;
        let cfg: UnifiedConfig = toml::from_str(src).unwrap();
        assert!(cfg.platform.is_runtime_reporting_active());
        assert_eq!(cfg.platform.runtime_id.as_deref(), Some("desktop-01"));
        assert_eq!(cfg.platform.workspace_id.as_deref(), Some("w1"));
        assert_eq!(cfg.platform.user_id.as_deref(), Some("u1"));
    }
}
