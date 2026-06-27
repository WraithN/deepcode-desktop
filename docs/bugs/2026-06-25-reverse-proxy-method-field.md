# Reverse Proxy /v1/messages Method 字段覆盖

## 现象

当 opencode serve 返回的 `/v1/messages` 响应中包含 `message.method` 字段且值与 `model` 相等时，reverse proxy（gatewayd）将其原样转发给前端。前端收到后将其视为 model 切换事件，导致 streaming 行为和 UI 表现异常。

## 根因

openclaude 风格的 `/v1/messages` 流式响应可能包含 `method` 字段（例如 `"method": "model"` 表示模型切换）。前端仅通过 `Method` 字段区分事件类型，当 `Method === "model"` 时，前端会触发 model 切换事件并修改 streaming 状态机的行为。

在 reverse proxy 中，`/v1/messages` 的 SSE 事件被逐行转发到 WebSocket，包括其 `method` 字段。如果该字段等于 `"model"`，前端会错误地视为模型切换。

## 解决方案

在 `apps/gatewayd/src/proxy.rs` 中，当解析 `/v1/messages` 的 SSE `data:` 行时：

- 如果 JSON 中的 `Method` 字段值等于 `"model"`，则将其清空（设为 `""`）
- 修改发生在 `parse_v1_data_line` 函数中，只影响转发到前端的消息，不影响后端处理

```rust
fn parse_v1_data_line(data: &str) -> Option<String> {
    let mut val: serde_json::Value = serde_json::from_str(data).ok()?;
    if let Some(obj) = val.as_object_mut() {
        if obj.get("Method").and_then(|v| v.as_str()) == Some("model") {
            obj.insert("Method".to_string(), serde_json::Value::String(String::new()));
        }
    }
    val.get("Method")
        .and_then(|v| v.as_str())
        .filter(|m| !m.is_empty())?;
    Some(val.to_string())
}
```

### 验证

- `cargo test -p opencode-plugin -p agent-core` 全部通过
- 手动 `curl` 测试确认 `"Method":"model"` 被清空为 `"Method":""`
- 构建无 warning
