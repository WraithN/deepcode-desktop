//! Build the Claude Code `settings.json` body from a [`UnifiedConfig`].
//!
//! Claude Code reads model + base url + api key from the `env` block and a
//! parallel `mcpServers` map. To stay non-destructive we only overwrite the
//! keys we own (listed in `MANAGED_KEYS`) and preserve any other user fields
//! found in the existing file.

use crate::constants::{MANAGED_KEYS_KEY, SENTINEL_KEY};
use crate::error::Result;
use dh_config::{McpServerConfig, ModelConfig, ProviderConfig, UnifiedConfig};
use serde_json::{json, Map, Value};

/// JSON keys this adapter owns.
pub const MANAGED_KEYS: &[&str] = &["env", "mcpServers"];

// ───── env keys ─────
const ENV_BASE_URL: &str = "ANTHROPIC_BASE_URL";
const ENV_API_KEY: &str = "ANTHROPIC_API_KEY";
const ENV_MODEL: &str = "ANTHROPIC_MODEL";
const ENV_DEFAULT_SONNET: &str = "ANTHROPIC_DEFAULT_SONNET_MODEL";
const ENV_DEFAULT_HAIKU: &str = "ANTHROPIC_DEFAULT_HAIKU_MODEL";
const ENV_DEFAULT_OPUS: &str = "ANTHROPIC_DEFAULT_OPUS_MODEL";

/// Builds the JSON document, preserving unmanaged keys from `existing`.
pub fn build(cfg: &UnifiedConfig, existing: Option<&Value>) -> Result<Value> {
    let mut root = base_object_from_existing(existing);
    root.insert(SENTINEL_KEY.to_string(), Value::Bool(true));
    root.insert(
        MANAGED_KEYS_KEY.to_string(),
        Value::Array(
            MANAGED_KEYS
                .iter()
                .map(|k| Value::String((*k).to_string()))
                .collect(),
        ),
    );
    root.insert("env".to_string(), build_env(cfg));
    root.insert("mcpServers".to_string(), build_mcp(cfg));
    Ok(Value::Object(root))
}

fn base_object_from_existing(existing: Option<&Value>) -> Map<String, Value> {
    let mut map = match existing {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    // Drop any previously-managed keys so we re-emit them deterministically.
    for k in MANAGED_KEYS {
        map.remove(*k);
    }
    map.remove(SENTINEL_KEY);
    map.remove(MANAGED_KEYS_KEY);
    map
}

fn build_env(cfg: &UnifiedConfig) -> Value {
    let mut env = Map::new();
    let model = cfg.default_model();
    let provider = model.and_then(|m| cfg.providers.get(&m.provider));

    if let Some(base) = provider.and_then(|p| p.base_url.as_ref()) {
        env.insert(ENV_BASE_URL.to_string(), Value::String(base.clone()));
    }
    if let Some(key) = provider.and_then(|p| p.api_key.as_ref()) {
        env.insert(ENV_API_KEY.to_string(), Value::String(key.clone()));
    }
    if let Some(m) = model {
        insert_model_env(&mut env, m);
    }
    Value::Object(env)
}

fn insert_model_env(env: &mut Map<String, Value>, model: &ModelConfig) {
    let name = Value::String(model.name.clone());
    env.insert(ENV_MODEL.to_string(), name.clone());
    // Mirror to all default tiers so Claude Code uses the user's model
    // regardless of which tier the agent invokes internally.
    env.insert(ENV_DEFAULT_SONNET.to_string(), name.clone());
    env.insert(ENV_DEFAULT_HAIKU.to_string(), name.clone());
    env.insert(ENV_DEFAULT_OPUS.to_string(), name);
}

fn build_mcp(cfg: &UnifiedConfig) -> Value {
    let mut servers = Map::new();
    for entry in cfg.mcp.iter().filter(|e| applies_to_claudecode(e)) {
        servers.insert(entry.name.clone(), build_one_mcp(entry));
    }
    Value::Object(servers)
}

fn applies_to_claudecode(entry: &McpServerConfig) -> bool {
    if !entry.enabled {
        return false;
    }
    entry.scopes.is_empty() || entry.scopes.iter().any(|s| s == "claudecode")
}

fn build_one_mcp(entry: &McpServerConfig) -> Value {
    let mut env = Map::new();
    for (k, v) in &entry.env {
        env.insert(k.clone(), Value::String(v.clone()));
    }
    json!({
        "command": entry.command,
        "args": entry.args,
        "env": Value::Object(env),
    })
}

// Allow building a settings document without referencing a specific provider
// (useful for tests that don't hit `default_model`).
#[allow(dead_code)]
fn provider_or_default<'a>(
    cfg: &'a UnifiedConfig,
    model: &ModelConfig,
) -> Option<&'a ProviderConfig> {
    cfg.providers.get(&model.provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn cfg_with_model() -> UnifiedConfig {
        let mut cfg = UnifiedConfig::default();
        cfg.providers.insert(
            "anthropic".into(),
            ProviderConfig {
                base_url: Some("https://api.deepseek.com/v1".into()),
                api_key: Some("sk-test".into()),
                extra: BTreeMap::new(),
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
    fn emits_env_block() {
        let cfg = cfg_with_model();
        let v = build(&cfg, None).unwrap();
        let env = &v["env"];
        assert_eq!(env[ENV_BASE_URL], "https://api.deepseek.com/v1");
        assert_eq!(env[ENV_API_KEY], "sk-test");
        assert_eq!(env[ENV_MODEL], "deepseek-chat");
        assert_eq!(env[ENV_DEFAULT_SONNET], "deepseek-chat");
    }

    #[test]
    fn preserves_unmanaged_keys() {
        let mut existing = json!({
            "permissions": { "allow": ["Read"] },
            "env": { "OLD": "value" },
            SENTINEL_KEY: true,
            MANAGED_KEYS_KEY: ["env", "mcpServers"]
        });
        existing.as_object_mut().unwrap();
        let cfg = cfg_with_model();
        let v = build(&cfg, Some(&existing)).unwrap();
        assert!(v.get("permissions").is_some());
        // env is regenerated; the old OLD key must be gone.
        assert!(v["env"].get("OLD").is_none());
    }

    #[test]
    fn includes_only_enabled_mcp_servers() {
        let mut cfg = cfg_with_model();
        cfg.mcp.push(McpServerConfig {
            name: "fs".into(),
            command: "npx".into(),
            args: vec!["@modelcontextprotocol/server-filesystem".into()],
            enabled: true,
            ..Default::default()
        });
        cfg.mcp.push(McpServerConfig {
            name: "off".into(),
            command: "x".into(),
            enabled: false,
            ..Default::default()
        });
        cfg.mcp.push(McpServerConfig {
            name: "opencode-only".into(),
            command: "y".into(),
            enabled: true,
            scopes: vec!["opencode".into()],
            ..Default::default()
        });
        let v = build(&cfg, None).unwrap();
        let servers = v["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("fs"));
        assert!(!servers.contains_key("off"));
        assert!(!servers.contains_key("opencode-only"));
    }
}
