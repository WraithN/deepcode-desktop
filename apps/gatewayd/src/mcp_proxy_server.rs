//! MCP proxy server endpoint (`POST /mcp`).
//!
//! gatewayd 在 agent 侧扮演 MCP server 角色，将 `McpRegistry` 聚合的工具
//! 通过单一 JSON-RPC 端点暴露给 agent。本模块手写 JSON-RPC 处理逻辑，
//! 不引入 MCP server SDK，保持 gatewayd 侧轻量、无状态。
use axum::{extract::State, http::StatusCode, response::Json};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

use super::ApiState;

/// MCP 协议版本（与 dh-core / crawler 协商一致的固定值）。
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// crawler 工具 `web_scrape.maxDepth` 参数的默认值。
///
/// 由 server.rs 启动时写入 `ApiState.crawler_max_depth`（默认值即本常量），
/// Task 8 之后由 dh-backend 拉取的真实配置覆盖。本模块在 `tools/list`
/// 响应中据此改写工具 schema 的 default 字段，保证 agent 看到的默认值
/// 与平台策略一致。
pub(crate) const MCP_DEFAULT_MAX_DEPTH: i64 = 2;

/// crawler 工具 `web_scrape` 的裸工具名（聚合前）。
const WEB_SCRAPE_TOOL_NAME: &str = "web_scrape";

/// `web_scrape` 带命名空间前缀后的后缀（含分隔符 `:`）。
///
/// `aggregate_tools` 会给工具名加 `{server}:` 前缀，crawler 的工具实际名为
/// `crawler:web_scrape`。用后缀匹配做到命名空间无关，避免硬编码 server 名。
const WEB_SCRAPE_TOOL_SUFFIX: &str = ":web_scrape";

/// 改写工具列表中 `web_scrape.maxDepth` 的 default 值。
///
/// `McpRegistry::aggregate_tools` 返回的工具 schema 由 crawler 原始声明，
/// 其 default 可能与平台策略不一致。本函数在序列化前原地修改 default，
/// 让 agent 在不显式传参时使用平台期望的爬取深度。
pub fn rewrite_tool_defaults(tools: &mut [Value], max_depth: i64) {
    for t in tools.iter_mut() {
        if tool_name_is_web_scrape(t) {
            rewrite_max_depth_default(t, max_depth);
        }
    }
}

/// 判断工具是否为 crawler 的 `web_scrape`。
///
/// 兼容两种形式：`aggregate_tools` 命名空间化后的 `crawler:web_scrape`
/// （及任何 `{namespace}:web_scrape`），以及未命名空间的裸 `web_scrape`
/// （手工构造的数据 / 未来非聚合场景）。
fn tool_name_is_web_scrape(tool: &Value) -> bool {
    let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
    name == WEB_SCRAPE_TOOL_NAME || name.ends_with(WEB_SCRAPE_TOOL_SUFFIX)
}

/// 就地改写单个 `web_scrape` 工具 schema 中 `maxDepth.default`。
///
/// 使用 `let-else` 提前返回避免深层嵌套；任一层级缺失即视为 schema 不完整，
/// 静默跳过（crawler 声明的 schema 始终包含完整路径，缺失仅出现在手工构造的测试数据中）。
fn rewrite_max_depth_default(tool: &mut Value, max_depth: i64) {
    let Some(schema) = tool.get_mut("inputSchema") else {
        return;
    };
    let Some(props) = schema.get_mut("properties") else {
        return;
    };
    let Some(md) = props.get_mut("maxDepth") else {
        return;
    };
    md["default"] = json!(max_depth);
}

/// `/mcp` 端点主处理器：实现 MCP `initialize` / `tools/list` / `tools/call`
/// 三个核心方法，其余方法返回 JSON-RPC `-32601 method not found`。
///
/// 无状态：每次请求独立处理，不维护会话句柄。`tools/list` 与 `tools/call`
/// 通过 `state.mcp_registry` 转发到 crawler（HttpTransport）。
pub async fn mcp_endpoint(
    State(state): State<ApiState>,
    Json(req): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => Ok(Json(json!({
            "jsonrpc": "2.0", "id": id,
            "result": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "gatewayd-mcp-proxy", "version": env!("CARGO_PKG_VERSION") }
            }
        }))),
        "notifications/initialized" => Ok(Json(Value::Null)),
        "tools/list" => {
            let registry = state
                .mcp_registry
                .as_ref()
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            let r = registry.lock().await;
            // aggregate_tools 返回 Vec<Tool>（强类型），需逐个序列化为 Value
            // 以便原地改写 schema default。
            let mut tools: Vec<Value> = r
                .aggregate_tools()
                .await
                .into_iter()
                .map(serde_json::to_value)
                .filter_map(Result::ok)
                .collect();
            rewrite_tool_defaults(
                &mut tools,
                state.crawler_max_depth.load(Ordering::Relaxed),
            );
            Ok(Json(json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools } })))
        }
        "tools/call" => {
            let name = req
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let args = req
                .get("params")
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(json!({}));
            let registry = state
                .mcp_registry
                .as_ref()
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            let r = registry.lock().await;
            match r.call_tool(name, args).await {
                Ok(result) => Ok(Json(json!({ "jsonrpc": "2.0", "id": id, "result": result }))),
                Err(e) => Ok(Json(json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32603, "message": e.to_string() }
                }))),
            }
        }
        _ => Ok(Json(json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": "method not found" }
        }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个 `web_scrape` 风格的 tool schema（含 `maxDepth.default`），
    /// 用于验证 `rewrite_tool_defaults` 的命名空间匹配行为。
    fn web_scrape_tool(name: &str, default: i64) -> Value {
        json!({
            "name": name,
            "inputSchema": {
                "properties": {
                    "maxDepth": { "type": "integer", "default": default }
                }
            }
        })
    }

    #[test]
    fn rewrite_tool_defaults_rewrites_namespaced_and_bare_web_scrape() {
        let mut tools = vec![
            web_scrape_tool("crawler:web_scrape", 0),
            web_scrape_tool("web_scrape", 0),
            web_scrape_tool("other:web_scrape", 0),
            // 非 web_scrape 工具，不应被改写。
            web_scrape_tool("crawler:web_search", 0),
        ];
        rewrite_tool_defaults(&mut tools, MCP_DEFAULT_MAX_DEPTH);

        assert_eq!(
            tools[0]["inputSchema"]["properties"]["maxDepth"]["default"],
            json!(MCP_DEFAULT_MAX_DEPTH),
            "聚合后的 crawler:web_scrape 应被改写 default"
        );
        assert_eq!(
            tools[1]["inputSchema"]["properties"]["maxDepth"]["default"],
            json!(MCP_DEFAULT_MAX_DEPTH),
            "未命名空间的裸 web_scrape 应被改写 default"
        );
        assert_eq!(
            tools[2]["inputSchema"]["properties"]["maxDepth"]["default"],
            json!(MCP_DEFAULT_MAX_DEPTH),
            "其它命名空间的 :web_scrape 应被改写 default（命名空间无关）"
        );
        assert_eq!(
            tools[3]["inputSchema"]["properties"]["maxDepth"]["default"],
            json!(0),
            "非 web_scrape 工具不应被改写"
        );
    }
}
