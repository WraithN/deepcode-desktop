# Token 统计与 gatewayd stats 指令实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 gatewayd 中拦截 LLM 响应解析 usage 字段（失败时按模型预估），写入 Response 审计日志；在 CLI 新增 `dh gatewayd stats` 命令展示上行/下行 Token 统计。

**Architecture:** gatewayd 的请求处理函数在返回响应前，通过 `response.into_parts()` 读取 body bytes，解析 JSON/SSE 中的 `usage` 字段，创建 `Direction::Response` 审计日志写入 SQLite。CLI 直接查询本地 `gatewayd.db` 做 SQL 聚合，默认表格输出，支持 `--json`。

**Tech Stack:** Rust, axum, reqwest, rusqlite, serde_json, clap

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `crates/dh-core/src/models/response.rs` | 新增 `ModelTokenProfile` 和预估函数 |
| `crates/dh-core/src/models/mod.rs` | 导出新增类型 |
| `apps/gatewayd/src/main.rs` | 修改 `forward_openai`/`forward_anthropic` 保留 Content-Type；修改两个 handler 拦截响应体解析 usage |
| `apps/cli/src/commands/gatewayd.rs` | 新增 `Stats` 子命令和聚合查询逻辑 |
| `apps/cli/src/main.rs` | 注册 Stats 命令（如有需要） |
| `crates/dh-core/src/models/token_tests.rs` | 单元测试 |

---

## Task 1: 添加模型 Token 预估配置到 dh-core

**Files:**
- Modify: `crates/dh-core/src/models/response.rs`
- Modify: `crates/dh-core/src/models/mod.rs`

- [ ] **Step 1: 修改 response.rs，添加 ModelTokenProfile 和预估函数**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedResponse {
    pub id: String,
    pub session_id: String,
    pub model: String,
    pub content: String,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub session_id: String,
    pub delta: String,
    pub finish_reason: Option<String>,
}

/// 模型 Token 预估配置
#[derive(Debug, Clone)]
pub struct ModelTokenProfile {
    pub chars_per_token_chinese: f32,
    pub chars_per_token_other: f32,
    pub overhead_tokens: u32,
}

/// 内置模型预估配置表
pub fn resolve_model_profile(model: &str) -> ModelTokenProfile {
    const PROFILES: &[(&str, ModelTokenProfile)] = &[
        ("gpt-4o", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-4o-mini", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-4-turbo", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-4", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-3.5-turbo", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("claude-3-5-sonnet", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
        ("claude-3-opus", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
        ("claude-3-haiku", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
        ("deepseek-chat", ModelTokenProfile { chars_per_token_chinese: 1.4, chars_per_token_other: 3.8, overhead_tokens: 4 }),
        ("deepseek-coder", ModelTokenProfile { chars_per_token_chinese: 1.4, chars_per_token_other: 3.8, overhead_tokens: 4 }),
    ];

    for (key, profile) in PROFILES {
        if model == *key || model.starts_with(key) {
            return profile.clone();
        }
    }

    ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }
}

/// 按模型预估 Token 数
pub fn estimate_tokens(payload: &str, model: &str) -> u32 {
    let profile = resolve_model_profile(model);
    let chinese_chars = payload.chars().filter(|c| c.is_cjk()).count();
    let other_chars = payload.chars().count() - chinese_chars;

    (chinese_chars as f32 / profile.chars_per_token_chinese
        + other_chars as f32 / profile.chars_per_token_other)
        .ceil() as u32
        + profile.overhead_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_english() {
        let text = "Hello world this is a test message for token estimation.";
        let tokens = estimate_tokens(text, "gpt-4o");
        // ~54 chars / 4.0 = 13.5 + 3 overhead ≈ 17
        assert!(tokens >= 10 && tokens <= 25, "Expected ~17 tokens, got {}", tokens);
    }

    #[test]
    fn test_estimate_tokens_chinese() {
        let text = "这是一个中文测试消息，用于验证Token估算功能。";
        let tokens = estimate_tokens(text, "gpt-4o");
        // ~25 chinese chars / 1.5 = 16.7 + 3 overhead ≈ 20
        assert!(tokens >= 15 && tokens <= 30, "Expected ~20 tokens, got {}", tokens);
    }

    #[test]
    fn test_resolve_model_profile_exact() {
        let p = resolve_model_profile("gpt-4o");
        assert_eq!(p.chars_per_token_chinese, 1.5);
        assert_eq!(p.overhead_tokens, 3);
    }

    #[test]
    fn test_resolve_model_profile_prefix() {
        let p = resolve_model_profile("gpt-4o-2024-08-06");
        assert_eq!(p.chars_per_token_chinese, 1.5);
    }

    #[test]
    fn test_resolve_model_profile_fallback() {
        let p = resolve_model_profile("unknown-model");
        assert_eq!(p.chars_per_token_chinese, 1.5);
        assert_eq!(p.overhead_tokens, 3);
    }

    #[test]
    fn test_claude_profile() {
        let p = resolve_model_profile("claude-3-5-sonnet");
        assert_eq!(p.chars_per_token_chinese, 1.3);
        assert_eq!(p.overhead_tokens, 5);
    }
}
```

- [ ] **Step 2: 确认 mod.rs 已导出（通常已有 `pub use response::{...}`）**

检查 `crates/dh-core/src/models/mod.rs` 第 8 行：
```rust
pub use response::{StreamChunk, TokenUsage, UnifiedResponse};
```

修改为：
```rust
pub use response::{estimate_tokens, resolve_model_profile, ModelTokenProfile, StreamChunk, TokenUsage, UnifiedResponse};
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p dh-core`
Expected: 0 errors, tests compile

- [ ] **Step 4: 运行测试**

Run: `cargo test -p dh-core`
Expected: 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/dh-core/src/models/response.rs crates/dh-core/src/models/mod.rs
git commit -m "feat(core): add ModelTokenProfile and estimate_tokens for model-aware token estimation"
```

---

## Task 2: 修改 GatewayRouter 保留原始 Content-Type

**Files:**
- Modify: `apps/gatewayd/src/main.rs`

- [ ] **Step 1: 修改 `forward_openai` 方法**

找到 `forward_openai` 方法（约 line 170-203），修改响应构建部分：

```rust
async fn forward_openai(&self, provider: &str, body: String) -> Result<Response, anyhow::Error> {
    let (url, api_key) = match provider {
        "deepseek" => {
            let key = self.deepseek_api_key.as_ref()
                .ok_or_else(|| anyhow::anyhow!("DEEPSEEK_API_KEY not set"))?;
            ("https://api.deepseek.com/v1/chat/completions", key)
        }
        _ => {
            let key = self.openai_api_key.as_ref()
                .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
            ("https://api.openai.com/v1/chat/completions", key)
        }
    };

    info!("Forwarding {} request to {}", provider, url);

    let resp = self.client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;

    let status = resp.status();
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let bytes = resp.bytes().await?;

    let mut builder = Response::builder().status(status);
    builder = builder.header("Content-Type", content_type);
    let response = builder.body(Body::from(bytes))?;

    Ok(response)
}
```

- [ ] **Step 2: 修改 `forward_anthropic` 方法**

找到 `forward_anthropic` 方法（约 line 205-229），同样修改：

```rust
async fn forward_anthropic(&self, body: String) -> Result<Response, anyhow::Error> {
    let api_key = self.anthropic_api_key.as_ref()
        .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    info!("Forwarding request to Anthropic API");

    let resp = self.client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;

    let status = resp.status();
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let bytes = resp.bytes().await?;

    let mut builder = Response::builder().status(status);
    builder = builder.header("Content-Type", content_type);
    let response = builder.body(Body::from(bytes))?;

    Ok(response)
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p gatewayd`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add apps/gatewayd/src/main.rs
git commit -m "fix(gatewayd): preserve upstream Content-Type header in forwarded responses"
```

---

## Task 3: 在 gatewayd handler 中拦截响应并解析 usage

**Files:**
- Modify: `apps/gatewayd/src/main.rs`

- [ ] **Step 1: 在 main.rs 顶部添加必要的 use 语句**

在现有 use 语句下方添加：
```rust
use dh_core::estimate_tokens;
```

- [ ] **Step 2: 添加 usage 解析辅助函数**

在 `main.rs` 中 `AuditStorage` 实现之后（约 line 80 之后），添加以下函数：

```rust
/// 从 JSON 响应体中提取 usage
fn extract_usage_from_json(body: &[u8]) -> Option<dh_core::TokenUsage> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let usage = json.get("usage")?;
    Some(dh_core::TokenUsage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage.get("completion_tokens")?.as_u64()? as u32,
        total_tokens: usage.get("total_tokens")?.as_u64()? as u32,
    })
}

/// 从 SSE 流文本中提取最后一个 usage chunk
fn extract_usage_from_sse(text: &str) -> Option<dh_core::TokenUsage> {
    let mut last_usage: Option<dh_core::TokenUsage> = None;
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Some(usage) = extract_usage_from_json(data.as_bytes()) {
                last_usage = Some(usage);
            }
        }
    }
    last_usage
}

/// 创建 Response 审计日志并发送
fn log_response_audit(
    audit: &AuditLogger,
    session_id: String,
    request_id: String,
    provider: String,
    model: String,
    bytes: &[u8],
    request_body: &str,
) {
    let usage = extract_usage_from_json(bytes)
        .or_else(|| {
            let text = String::from_utf8_lossy(bytes);
            extract_usage_from_sse(&text)
        });

    let usage = match usage {
        Some(u) => u,
        None => {
            let response_text = String::from_utf8_lossy(bytes);
            dh_core::TokenUsage {
                prompt_tokens: estimate_tokens(request_body, &model),
                completion_tokens: estimate_tokens(&response_text, &model),
                total_tokens: 0,
            }
        }
    };

    let mut entry = dh_core::AuditLogEntry::new(
        session_id,
        request_id,
        dh_core::Direction::Response,
        provider,
        model,
    );
    entry.token_usage = Some(usage);
    entry.payload_size_bytes = bytes.len();
    entry.metadata = serde_json::json!({
        "token_source": if extract_usage_from_json(bytes).is_some() || extract_usage_from_sse(&String::from_utf8_lossy(bytes)).is_some() {
            "provider"
        } else {
            "estimated"
        }
    });
    audit.log(entry);
}
```

- [ ] **Step 3: 修改 `openai_chat_completions` handler**

找到 `openai_chat_completions` 函数（约 line 506），修改 `Ok(response)` 分支：

```rust
match state.router.forward_openai(provider, optimized_body.clone()).await {
    Ok(response) => {
        info!("Successfully forwarded request to {}", provider);
        let (parts, body) = response.into_parts();
        match axum::body::to_bytes(body, usize::MAX).await {
            Ok(bytes) => {
                log_response_audit(
                    &state.audit,
                    session_id.clone(),
                    unified.id.clone(),
                    provider.to_string(),
                    unified.model.clone(),
                    &bytes,
                    &optimized_body,
                );
                let body = axum::body::Body::from(bytes);
                axum::response::Response::from_parts(parts, body)
            }
            Err(e) => {
                error!("Failed to read response body: {}", e);
                (StatusCode::BAD_GATEWAY, "Gateway error: failed to read response").into_response()
            }
        }
    }
    Err(e) => {
        error!("Failed to forward request to {}: {}", provider, e);
        (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
    }
}
```

- [ ] **Step 4: 修改 `anthropic_messages` handler**

找到 `anthropic_messages` 函数（约 line 564），同样修改 `Ok(response)` 分支：

```rust
match state.router.forward_anthropic(optimized_body.clone()).await {
    Ok(response) => {
        info!("Successfully forwarded request to {}", provider);
        let (parts, body) = response.into_parts();
        match axum::body::to_bytes(body, usize::MAX).await {
            Ok(bytes) => {
                log_response_audit(
                    &state.audit,
                    session_id.clone(),
                    unified.id.clone(),
                    provider.to_string(),
                    unified.model.clone(),
                    &bytes,
                    &optimized_body,
                );
                let body = axum::body::Body::from(bytes);
                axum::response::Response::from_parts(parts, body)
            }
            Err(e) => {
                error!("Failed to read response body: {}", e);
                (StatusCode::BAD_GATEWAY, "Gateway error: failed to read response").into_response()
            }
        }
    }
    Err(e) => {
        error!("Failed to forward request to {}: {}", provider, e);
        (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
    }
}
```

- [ ] **Step 5: 编译验证**

Run: `cargo check -p gatewayd`
Expected: 0 errors

- [ ] **Step 6: Commit**

```bash
git add apps/gatewayd/src/main.rs
git commit -m "feat(gatewayd): intercept responses to parse usage and write Response audit logs"
```

---

## Task 4: 在 CLI 中添加 stats 子命令

**Files:**
- Modify: `apps/cli/src/commands/gatewayd.rs`

- [ ] **Step 1: 在 GatewaydCommands 枚举中添加 Stats 变体**

找到 `GatewaydCommands` 枚举（约 line 4-34），在 `Request` 变体之后添加：

```rust
/// View token usage statistics
Stats {
    #[arg(long)]
    session_id: Option<String>,
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    provider: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    json: bool,
},
```

- [ ] **Step 2: 在 run() 匹配中添加 Stats 分支**

找到 `run()` 函数中的匹配（约 line 36-221），在 `GatewaydCommands::Request` 分支之后添加：

```rust
GatewaydCommands::Stats { session_id, since, provider, model, json } => {
    let conn = open_db()?;
    let mut sql = String::from(
        "SELECT 
            COALESCE(SUM(prompt_tokens), 0),
            COALESCE(SUM(completion_tokens), 0),
            COALESCE(SUM(total_tokens), 0),
            COUNT(CASE WHEN direction = 'request' THEN 1 END),
            COUNT(CASE WHEN direction = 'response' THEN 1 END),
            COUNT(CASE WHEN metadata LIKE '%\"token_source\":\"estimated\"%' THEN 1 END)
         FROM audit_logs WHERE 1=1"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::new();

    if let Some(ref sid) = session_id {
        sql.push_str(" AND session_id = ?");
        params.push(sid);
    }
    if let Some(ref since_val) = since {
        sql.push_str(" AND timestamp >= ?");
        params.push(since_val);
    }
    if let Some(ref prov) = provider {
        sql.push_str(" AND provider = ?");
        params.push(prov);
    }
    if let Some(ref m) = model {
        sql.push_str(" AND model = ?");
        params.push(m);
    }

    let mut stmt = conn.prepare(&sql)?;
    let (prompt, completion, total, requests, responses, estimated): (
        i64, i64, i64, i64, i64, i64
    ) = stmt.query_row(rusqlite::params_from_iter(params), |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    })?;

    if json {
        println!("{}", serde_json::json!({
            "upstream_tokens": prompt,
            "downstream_tokens": completion,
            "total_tokens": total,
            "total_requests": requests,
            "total_responses": responses,
            "estimated_count": estimated,
            "filters": {
                "session_id": session_id,
                "since": since,
                "provider": provider,
                "model": model,
            }
        }));
    } else {
        println!("┌──────────────────────┬─────────┬───────────┐");
        println!("│ {:<20} │ {:>7} │ {:<9} │", "Metric", "Value", "Source");
        println!("├──────────────────────┼─────────┼───────────┤");

        let source = if estimated == 0 {
            "provider"
        } else if estimated >= responses {
            "estimated"
        } else {
            "mixed"
        };

        println!("│ {:<20} │ {:>7} │ {:<9} │", "Upstream Tokens", prompt, source);
        println!("│ {:<20} │ {:>7} │ {:<9} │", "Downstream Tokens", completion, source);
        println!("│ {:<20} │ {:>7} │ {:<9} │", "Total Tokens", total, "—");
        println!("│ {:<20} │ {:>7} │ {:<9} │", "Total Requests", requests, "—");
        println!("│ {:<20} │ {:>7} │ {:<9} │", "Total Responses", responses, "—");
        println!("└──────────────────────┴─────────┴───────────┘");

        if estimated > 0 {
            println!("\n* {} records used estimated token counts", estimated);
        }

        let mut filters = Vec::new();
        if let Some(sid) = &session_id {
            filters.push(format!("session_id={}", sid));
        }
        if let Some(s) = &since {
            filters.push(format!("since={}", s));
        }
        if let Some(p) = &provider {
            filters.push(format!("provider={}", p));
        }
        if let Some(m) = &model {
            filters.push(format!("model={}", m));
        }
        if !filters.is_empty() {
            println!("Filters: {}", filters.join(", "));
        }
    }
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p cli`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add apps/cli/src/commands/gatewayd.rs
git commit -m "feat(cli): add gatewayd stats command with table and JSON output"
```

---

## Task 5: 编译清理和最终验证

- [ ] **Step 1: 运行 cargo check 全项目**

Run: `cargo check --workspace`
Expected: 0 errors

- [ ] **Step 2: 运行 cargo test**

Run: `cargo test -p dh-core`
Expected: 5 tests PASS

Run: `cargo test -p gatewayd`
Expected: 4 tests PASS (reporter tests) + 0 new failures

Run: `cargo test -p cli`
Expected: 0 tests (cli has no tests currently)

- [ ] **Step 3: 运行 cargo clippy**

Run: `cargo clippy -p gatewayd -- -D warnings`
Expected: 0 warnings in our code (existing warnings in pre-existing code acceptable)

Run: `cargo clippy -p cli -- -D warnings`
Expected: 0 warnings in our code

- [ ] **Step 4: 最终 commit**

```bash
git add .
git commit -m "feat(token-stats): complete token usage tracking and stats command

- Add model-aware token estimation (ModelTokenProfile)
- Intercept LLM responses to parse usage fields
- Write Direction::Response audit logs with token data
- Add dh gatewayd stats CLI command with filters
- Support table and JSON output formats"
```

---

## 自审清单

### 1. Spec 覆盖度

| Spec 需求 | 对应 Task |
|-----------|-----------|
| 拦截非流式响应解析 usage | Task 3 |
| 拦截流式响应解析 usage (SSE) | Task 3 (`extract_usage_from_sse`) |
| 无 usage 时按模型预估 | Task 1 + Task 3 |
| CLI stats 表格输出 | Task 4 |
| CLI stats JSON 输出 | Task 4 (`--json`) |
| 多维度过滤 | Task 4 (`--session-id`, `--since`, `--provider`, `--model`) |
| 标记 estimated 数据 | Task 3 (metadata) + Task 4 (表格提示) |

**无遗漏。**

### 2. 占位符扫描
- [x] 无 "TBD" / "TODO" / "implement later"
- [x] 所有代码片段完整可直接使用
- [x] 所有命令含预期输出

### 3. 类型一致性
- [x] `TokenUsage` 在 dh-core 和 gatewayd 中使用一致
- [x] `AuditLogEntry` 构造参数在各处一致
- [x] SQL 列名与 schema 定义一致

---

## 执行交接

**实现计划已保存至 `docs/superpowers/plans/2026-06-10-token-stats.md`**

两种执行方式：

**1. Subagent-Driven（推荐）** — 每个 Task 派一个独立子代理执行，我逐任务审查结果

**2. Inline Execution** — 我在当前会话中按 Task 顺序直接执行

你希望用哪种方式执行？
