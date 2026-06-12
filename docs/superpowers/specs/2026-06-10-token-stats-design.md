# Token 统计与 gatewayd stats 指令设计文档

> **日期**: 2026-06-10
> **主题**: gatewayd 上行/下行 Token 统计与 CLI stats 展示
> **状态**: 已确认

---

## 1. 需求概述

### 1.1 背景

gatewayd 目前已具备审计日志（`audit_logs`）功能，但仅记录了请求（`Direction::Request`）数据，且 `token_usage` 字段始终为 `None`。LLM 提供商的响应中包含精确的 `usage` 字段（`prompt_tokens`、`completion_tokens`、`total_tokens`），但 gatewayd 当前将响应直接透传给客户端，未做解析。

### 1.2 目标

1. 在 gatewayd 中拦截 LLM 响应（非流式 + 流式），解析 `usage` 字段并写入 `Direction::Response` 审计日志
2. 当提供商未返回 `usage` 时，使用按模型校准的智能预估算法估算 Token 数
3. 在 CLI 中新增 `dh gatewayd stats` 命令，聚合展示上行/下行 Token 统计
4. 支持多维度过滤（session、时间、provider、model）和多种输出格式（表格/JSON）

### 1.3 非目标

- 不实现 Token Bucket 限流或配额管理
- 不实现分布式计数器（单实例网关）
- 不维护本地 tokenizer 库做精确预计算
- 不实现成本加权统计（美元换算）

---

## 2. 架构设计

### 2.1 模块边界

```
apps/gatewayd/src/main.rs
├── 请求处理（openai_chat_completions / anthropic_messages）
│   ├── 现有：创建 Request 审计日志
│   └── 新增：响应拦截 + usage 解析 + Response 审计日志
│
apps/cli/src/commands/gatewayd.rs
├── 现有：Start / Stop / Status / Logs / Session / Request
└── 新增：Stats 子命令
│
crates/dh-db/src/reporter_db.rs 或新增 stats.rs
├── 新增：TokenStats 聚合查询
│
crates/dh-core/src/models/
├── TokenUsage（已有）
└── 新增：ModelTokenProfile（预估配置）
```

### 2.2 数据流

```
客户端 POST /v1/chat/completions
        │
        ▼
gatewayd 创建 AuditLogEntry(Request)
        │
        ▼
转发到 OpenAI/Anthropic API
        │
        ▼
接收响应
├─► 非流式：完整 JSON body
└─► 流式：SSE stream
        │
        ▼
解析 usage 字段
├─► 成功：使用 provider 返回的精确值
└─► 失败：按模型预估 Token 数
        │
        ▼
创建 AuditLogEntry(Response)
        │
        ▼
AuditStorage 写入 SQLite
        │
        ▼
CLI: dh gatewayd stats
        │
        ▼
SQL 聚合查询 → 表格 / JSON 输出
```

---

## 3. 响应拦截与 Token 解析

### 3.1 非流式响应

在 `openai_chat_completions` 和 `anthropic_messages` 中，当前代码返回响应前插入解析逻辑：

```rust
let (parts, body) = resp.into_parts();
let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap_or_default();

// 解析 usage
let usage = extract_usage_from_json(&bytes)
    .or_else(|| estimate_token_usage(&bytes, &model));

// 写入 Response 审计日志
let mut entry = AuditLogEntry::new(
    session_id.clone(),
    request_id.clone(),
    Direction::Response,
    provider.to_string(),
    model.clone(),
);
entry.token_usage = usage;
entry.payload_size_bytes = bytes.len();
entry.metadata = json!({"token_source": if usage.is_some() { "provider" } else { "estimated" }});
state.audit.log(entry);

// 重新组装响应返回
let body = axum::body::Body::from(bytes);
Ok(axum::response::Response::from_parts(parts, body))
```

### 3.2 流式响应（SSE）

流式响应通过 `Sse::new(stream)` 发送。在 stream mapping 中捕获最后一个 chunk 的 `usage`：

```rust
let mut usage: Option<TokenUsage> = None;
let mapped_stream = stream.map(move |chunk| {
    if let Ok(ref text) = chunk {
        if let Some(u) = extract_usage_from_sse_chunk(text) {
            usage = Some(u);
        }
    }
    chunk
});

// 后台任务：stream 结束后写入审计日志
tokio::spawn(async move {
    // 等待 stream consumer 完成
    // 然后使用 usage（或预估）写入 audit log
});
```

**SSE usage chunk 示例（OpenAI）：**
```
data: {"usage":{"prompt_tokens":150,"completion_tokens":300,"total_tokens":450},"choices":[]}
```

### 3.3 Usage 解析函数

```rust
fn extract_usage_from_json(body: &[u8]) -> Option<TokenUsage> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let usage = json.get("usage")?;
    Some(TokenUsage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage.get("completion_tokens")?.as_u64()? as u32,
        total_tokens: usage.get("total_tokens")?.as_u64()? as u32,
    })
}
```

---

## 4. 智能预估（无 usage 时）

### 4.1 模型级预估配置

```rust
pub struct ModelTokenProfile {
    pub chars_per_token_chinese: f32,
    pub chars_per_token_other: f32,
    pub overhead_tokens: u32,
}

// 配置表（可扩展）
const MODEL_PROFILES: &[(&str, ModelTokenProfile)] = &[
    ("gpt-4o",            ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
    ("gpt-4o-mini",       ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
    ("gpt-4-turbo",       ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
    ("gpt-4",             ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
    ("gpt-3.5-turbo",     ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
    ("claude-3-5-sonnet", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
    ("claude-3-opus",     ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
    ("claude-3-haiku",    ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
    ("deepseek-chat",     ModelTokenProfile { chars_per_token_chinese: 1.4, chars_per_token_other: 3.8, overhead_tokens: 4 }),
    ("deepseek-coder",    ModelTokenProfile { chars_per_token_chinese: 1.4, chars_per_token_other: 3.8, overhead_tokens: 4 }),
];
```

### 4.2 预估函数

```rust
fn estimate_tokens(payload: &str, model: &str) -> u32 {
    let profile = resolve_model_profile(model);
    
    let chinese_chars = payload.chars().filter(|c| c.is_cjk()).count();
    let other_chars = payload.chars().count() - chinese_chars;
    
    (chinese_chars as f32 / profile.chars_per_token_chinese
        + other_chars as f32 / profile.chars_per_token_other)
        .ceil() as u32
        + profile.overhead_tokens
}

fn resolve_model_profile(model: &str) -> &'static ModelTokenProfile {
    for (key, profile) in MODEL_PROFILES {
        if model == *key || model.starts_with(key) {
            return profile;
        }
    }
    // 默认值
    &ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }
}
```

### 4.3 预估精度

| 模型 | 预估误差 |
|------|---------|
| gpt-4o / gpt-4 | ±10-15% |
| claude-3-x | ±10-15% |
| deepseek-chat | ±12-18% |
| 未知/默认 | ±15-25% |

---

## 5. Stats 聚合查询

### 5.1 数据模型

```rust
pub struct TokenStats {
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
    pub total_requests: i64,
    pub total_responses: i64,
    pub estimated_count: i64,
}

pub struct TokenStatsFilter {
    pub session_id: Option<String>,
    pub since: Option<String>,      // ISO 8601
    pub provider: Option<String>,
    pub model: Option<String>,
}
```

### 5.2 SQL 聚合查询

```sql
SELECT 
    COALESCE(SUM(prompt_tokens), 0) as total_prompt,
    COALESCE(SUM(completion_tokens), 0) as total_completion,
    COALESCE(SUM(total_tokens), 0) as total_tokens,
    COUNT(CASE WHEN direction = 'request' THEN 1 END) as total_requests,
    COUNT(CASE WHEN direction = 'response' THEN 1 END) as total_responses,
    COUNT(CASE WHEN metadata LIKE '%"token_source":"estimated"%' THEN 1 END) as estimated_count
FROM audit_logs
WHERE 1=1
  AND session_id = ?          -- 可选
  AND timestamp >= ?          -- 可选
  AND provider = ?            -- 可选
  AND model = ?               -- 可选
```

---

## 6. CLI 设计

### 6.1 命令定义

```rust
pub enum GatewaydCommands {
    // ... 现有命令 ...
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
}
```

### 6.2 表格输出（默认）

```
┌──────────────────────┬─────────┬───────────┐
│ Metric               │ Value   │ Source    │
├──────────────────────┼─────────┼───────────┤
│ Upstream Tokens      │ 15,420  │ provider  │
│ Downstream Tokens    │ 8,200*  │ estimated │
│ Total Tokens         │ 23,620  │ mixed     │
│ Total Requests       │ 42      │ —         │
│ Total Responses      │ 40      │ —         │
└──────────────────────┴─────────┴───────────┘

* 部分数据为智能预估（15 条记录未返回 usage）
Filters: session_id=abc123, since=2026-06-01T00:00:00Z
```

### 6.3 JSON 输出（`--json`）

```json
{
  "upstream_tokens": 15420,
  "downstream_tokens": 8200,
  "total_tokens": 23620,
  "total_requests": 42,
  "total_responses": 40,
  "estimated_count": 15,
  "filters": {
    "session_id": "abc123",
    "since": "2026-06-01T00:00:00Z"
  }
}
```

---

## 7. 错误处理

| 场景 | 处理策略 |
|------|---------|
| 提供商未返回 usage | 使用模型预估，标记 `token_source: estimated` |
| 响应解析失败 | 记录 error log，跳过 usage 提取，返回原始响应 |
| 流式响应无 usage chunk | 使用预估，不阻塞流式传输 |
| audit 通道满 | `unbounded_channel`，不会阻塞 |
| stats 查询无 Response 日志 | 显示 `total_responses = 0`，提示可能未启用响应记录 |
| DB 文件被占用 | CLI 报错提示 gatewayd 可能正在运行 |

---

## 8. 测试策略

| 测试目标 | 方法 |
|---------|------|
| `extract_usage_from_json` | 单元测试：含/不含 usage 的 JSON |
| `extract_usage_from_sse_chunk` | 单元测试：各种 SSE chunk 格式 |
| `estimate_tokens` | 单元测试：中英文混合文本，不同模型 |
| `resolve_model_profile` | 单元测试：精确匹配、前缀匹配、回退 |
| `get_token_stats` | 集成测试：内存 SQLite，插入 mock 数据，验证聚合 |
| CLI 表格输出 | 手动测试：`dh gatewayd stats` |
| CLI JSON 输出 | 手动测试：`dh gatewayd stats --json` |

---

## 9. 验收标准

1. [ ] 非流式请求的 usage 被正确解析并写入 audit_logs
2. [ ] 流式请求的 usage 从 SSE 最后一个 chunk 提取并写入 audit_logs
3. [ ] 无 usage 时，按模型智能预估 Token 数，并标记 estimated
4. [ ] `dh gatewayd stats` 默认显示表格，包含上行/下行/总计 Token 数
5. [ ] `dh gatewayd stats --json` 输出 JSON 格式
6. [ ] `--session-id`、`--since`、`--provider`、`--model` 过滤生效
7. [ ] `cargo test` 中新增单元测试全部通过
8. [ ] `cargo check` 0 warnings（reporter 相关代码）

---

## 10. 未来扩展点

- **Token Bucket 限流**：基于实时 Token 消耗做配额控制
- **成本加权统计**：按模型价格换算为美元成本
- **本地 tokenizer**：集成 tiktoken 做精确预计算
- **远程 stats API**：Admin API `GET /admin/stats` 端点
