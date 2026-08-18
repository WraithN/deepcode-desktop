//! Build the OpenCode `opencode.json` body from a [`UnifiedConfig`].
//!
//! OpenCode 通过 `mcp` 键声明 MCP server，每个条目按连接类型区分：
//! - 本地（stdio）：`{ "type": "local", "command": [..], "environment": {..} }`
//! - 远程（http）：`{ "type": "remote", "url": "..." }`
//!
//! 与 claudecode adapter 一致，为保持非破坏性，仅覆盖本 adapter 管理的键
//! （`MANAGED_KEYS`），其余用户字段保留。当前仅管理 `mcp` 段。

use crate::builtin;
use crate::constants::{MANAGED_KEYS_KEY, SENTINEL_KEY};
use crate::error::Result;
use dh_config::{McpServerConfig, TransportKindCfg, UnifiedConfig};
use serde_json::{json, Map, Value};

/// JSON keys this adapter owns.
pub const MANAGED_KEYS: &[&str] = &["mcp"];

/// opencode 本地（stdio）MCP server 的 type 值。
const MCP_TYPE_LOCAL: &str = "local";
/// opencode 远程（http）MCP server 的 type 值。
const MCP_TYPE_REMOTE: &str = "remote";
/// opencode 本地 MCP server 的 environment 字段名。
const MCP_KEY_ENVIRONMENT: &str = "environment";

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
    root.insert("mcp".to_string(), build_mcp(cfg));
    Ok(Value::Object(root))
}

fn base_object_from_existing(existing: Option<&Value>) -> Map<String, Value> {
    let mut map = match existing {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    for k in MANAGED_KEYS {
        map.remove(*k);
    }
    map.remove(SENTINEL_KEY);
    map.remove(MANAGED_KEYS_KEY);
    map
}

fn build_mcp(cfg: &UnifiedConfig) -> Value {
    let mut servers = Map::new();
    let builtin = builtin::builtin_mcp_servers(cfg);
    for entry in cfg
        .mcp
        .iter()
        .chain(builtin.iter())
        .filter(|e| applies_to_opencode(e))
    {
        servers.insert(entry.name.clone(), build_one_mcp(entry));
    }
    Value::Object(servers)
}

fn applies_to_opencode(entry: &McpServerConfig) -> bool {
    if !entry.enabled {
        return false;
    }
    entry.scopes.is_empty() || entry.scopes.iter().any(|s| s == "opencode")
}

fn build_one_mcp(entry: &McpServerConfig) -> Value {
    // Http transport：opencode 远程 MCP server 使用 `type = "remote"` + `url`。
    if entry.transport == TransportKindCfg::Http {
        let url = entry.url.clone().unwrap_or_default();
        return json!({ "type": MCP_TYPE_REMOTE, "url": url });
    }
    // Stdio transport：opencode 本地 MCP server 使用 command 数组 + environment。
    let mut command = Vec::with_capacity(1 + entry.args.len());
    command.push(entry.command.clone());
    command.extend(entry.args.iter().cloned());
    let mut server = json!({ "type": MCP_TYPE_LOCAL, "command": command });
    if !entry.env.is_empty() {
        let mut env = Map::new();
        for (k, v) in &entry.env {
            env.insert(k.clone(), Value::String(v.clone()));
        }
        server[MCP_KEY_ENVIRONMENT] = Value::Object(env);
    }
    server
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_config::TransportKindCfg;

    #[test]
    fn renders_stdio_and_http_mcp() {
        let mut cfg = UnifiedConfig::default();
        cfg.mcp.push(McpServerConfig {
            name: "fs".into(),
            command: "npx".into(),
            args: vec!["@modelcontextprotocol/server-filesystem".into()],
            enabled: true,
            ..Default::default()
        });
        cfg.mcp.push(McpServerConfig {
            name: "remote".into(),
            transport: TransportKindCfg::Http,
            url: Some("https://example.com/mcp".into()),
            enabled: true,
            ..Default::default()
        });

        let v = build(&cfg, None).unwrap();
        let mcp = v["mcp"].as_object().unwrap();

        assert_eq!(mcp["fs"]["type"], "local");
        assert_eq!(
            mcp["fs"]["command"],
            serde_json::json!(["npx", "@modelcontextprotocol/server-filesystem"])
        );

        assert_eq!(mcp["remote"]["type"], "remote");
        assert_eq!(mcp["remote"]["url"], "https://example.com/mcp");
    }

    #[test]
    fn includes_only_enabled_and_scoped_mcp_servers() {
        let mut cfg = UnifiedConfig::default();
        cfg.mcp.push(McpServerConfig {
            name: "off".into(),
            command: "x".into(),
            enabled: false,
            ..Default::default()
        });
        cfg.mcp.push(McpServerConfig {
            name: "claudecode-only".into(),
            command: "y".into(),
            enabled: true,
            scopes: vec!["claudecode".into()],
            ..Default::default()
        });
        cfg.mcp.push(McpServerConfig {
            name: "gatewayd".into(),
            transport: TransportKindCfg::Http,
            url: Some("http://127.0.0.1:2346/mcp".into()),
            enabled: true,
            ..Default::default()
        });

        let v = build(&cfg, None).unwrap();
        let mcp = v["mcp"].as_object().unwrap();
        assert!(!mcp.contains_key("off"));
        assert!(!mcp.contains_key("claudecode-only"));
        // 内置 gatewayd 条目自动注入为 remote 类型。
        assert_eq!(mcp["gatewayd"]["type"], "remote");
        assert_eq!(mcp["gatewayd"]["url"], "http://127.0.0.1:2346/mcp");
    }

    #[test]
    fn preserves_unmanaged_keys() {
        let existing = json!({ "model": "anthropic/claude-sonnet", "mcp": { "old": {} } });
        let cfg = UnifiedConfig::default();
        let v = build(&cfg, Some(&existing)).unwrap();
        assert_eq!(v["model"], "anthropic/claude-sonnet");
        // mcp 段被重建，旧的 old 键应被移除。
        assert!(v["mcp"].get("old").is_none());
    }
}
