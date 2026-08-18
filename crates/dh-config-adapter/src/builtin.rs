//! 内置 MCP server 条目，渲染 agent 配置时注入。
//!
//! gatewayd 将聚合的 MCP 工具通过 `/mcp` 代理端点暴露给 agent（见 gatewayd
//! `mcp_proxy_server.rs`）。agent 需要在自身配置里声明一条指向该端点的 MCP
//! server 才能自动连接。本模块提供该内置条目的定义，供 claudecode / opencode
//! 等 adapter 在渲染时统一注入。

use dh_config::{McpServerConfig, TransportKindCfg, UnifiedConfig};

/// 内置 gatewayd MCP server 的名字。
pub const GATEWAYD_MCP_NAME: &str = "gatewayd";

/// 内置 gatewayd MCP server 的 URL。
///
/// gatewayd 的 `/mcp` 代理端点在 admin 端口（默认 2346）上注册，
/// 而非 API 端口（默认 2345），因此 URL 使用 2346。
pub const GATEWAYD_MCP_URL: &str = "http://127.0.0.1:2346/mcp";

/// 返回渲染 agent 配置时应注入的内置 MCP 条目。
///
/// 若用户配置中已存在同名条目则返回空（避免重复注入），否则返回一条指向
/// gatewayd `/mcp` 代理端点的 Http 条目，`scopes` 为空表示对所有 adapter 生效。
pub fn builtin_mcp_servers(cfg: &UnifiedConfig) -> Vec<McpServerConfig> {
    if cfg.mcp.iter().any(|e| e.name == GATEWAYD_MCP_NAME) {
        return Vec::new();
    }
    vec![McpServerConfig {
        name: GATEWAYD_MCP_NAME.into(),
        transport: TransportKindCfg::Http,
        url: Some(GATEWAYD_MCP_URL.into()),
        enabled: true,
        scopes: Vec::new(),
        ..Default::default()
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_gatewayd_entry_when_absent() {
        let cfg = UnifiedConfig::default();
        let entries = builtin_mcp_servers(&cfg);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, GATEWAYD_MCP_NAME);
        assert_eq!(entries[0].transport, TransportKindCfg::Http);
        assert_eq!(entries[0].url.as_deref(), Some(GATEWAYD_MCP_URL));
        assert!(entries[0].enabled);
        assert!(entries[0].scopes.is_empty());
    }

    #[test]
    fn skips_when_user_already_defined_gatewayd() {
        let mut cfg = UnifiedConfig::default();
        cfg.mcp.push(McpServerConfig {
            name: GATEWAYD_MCP_NAME.into(),
            transport: TransportKindCfg::Http,
            url: Some("http://custom/mcp".into()),
            ..Default::default()
        });
        assert!(builtin_mcp_servers(&cfg).is_empty());
    }
}
