# 异步会话日志上报设计文档

> **日期**: 2026-06-10
> **主题**: gatewayd 审计日志异步上报至远程 OTLP 端点
> **状态**: 已确认

---

## 1. 需求概述

### 1.1 背景

gatewayd 已具备完善的本地审计日志（`audit_logs`）功能，完整记录 LLM 请求/响应内容、策略触发、RTK 优化等信息。但在企业级场景下，需要将这些审计数据异步上报至远程可观测平台（如 Jaeger、Grafana、Datadog），以便集中分析、合规审计和异常检测。

### 1.2 目标

实现一个独立的异步上报模块，将 `audit_logs` 表中的数据通过 **OpenTelemetry OTLP/HTTP** 协议批量上报至远程端点，支持：

- 本地 SQLite 缓冲和断网恢复
- 批量发送（batch + flush interval）
- 网络失败重传（指数退避）
- 数据脱敏（可选 SHA-256 哈希）
- 运行时配置热更新

### 1.3 非目标

- 不上报 `session_logs`（运行时 DEBUG/INFO 日志）
- 不实现 gatewayd 自身的 metrics/traces（可未来扩展）
- 不上报 Desktop 端的本地会话日志

---

## 2. 架构设计

### 2.1 模块边界

```
apps/gatewayd/src/
├── main.rs              # 初始化 reporter 模块
├── audit.rs             # 现有：本地 audit_logs 写入（不感知 reporter）
├── mcp_aggregator.rs    # 现有
└── reporter/            # 新增：异步上报模块
    ├── mod.rs           # 模块导出、ReporterHandle、启动/关闭流程
    ├── config.rs        # ReporterConfig 配置结构
    ├── exporter.rs      # AuditLogExporter（LogBatchExporter 实现）
    ├── queue.rs         # SQLite 失败队列 CRUD + cursor 管理
    ├── transform.rs     # audit_log row → OTLP LogRecord 映射
    └── poller.rs        # 扫描 audit_logs 新条目并提交给 processor
```

**解耦原则：**
- `audit.rs` 保持纯本地写入职责，**不感知**远程上报是否存在
- `reporter/` 独立读取 `audit_logs` 表，通过 `last_sync_id` 游标追踪进度
- 数据库表作为天然的消息队列，避免引入外部 MQ

### 2.2 数据流

```
audit.rs 写入本地 audit_logs
        │
        ▼
[ SQLite audit_logs 表 ]
        │
        ▼
reporter::poller 定期扫描 (id > last_sync_id)
        │
        ▼
transform.rs: audit_log row → Vec<LogData>
        │
        ▼
opentelemetry_sdk::BatchLogProcessor (内存批处理)
        │
        ▼
自定义 exporter.rs:
    ├─ 脱敏处理（sanitize_content 为 true 时）
    ├─ 构造 OTLP ExportLogsServiceRequest
    ├─ HTTP POST 至 endpoint
    │
    ├─ 成功 → 更新 reporter_cursor.last_sync_id
    │
    └─ 失败 → 序列化后写入 reporter_queue，由 retry_worker 重试
```

---

## 3. 数据模型

### 3.1 配置模型

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ReporterConfig {
    /// 是否启用上报（默认 false）
    pub enabled: bool,

    /// OTLP/HTTP 端点，例如 http://localhost:4318/v1/logs
    pub endpoint: String,

    /// API Key，写入 Authorization: Bearer <api_key>
    pub api_key: Option<String>,

    /// 批量大小（默认 100）
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// flush 间隔秒数（默认 30）
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,

    /// 是否对 content / response 做哈希脱敏（默认 false）
    #[serde(default)]
    pub sanitize_content: bool,

    /// 最大重试次数（默认 10）
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}
```

**配置来源优先级（从高到低）：**
1. Admin API `PUT /admin/config/reporter` 运行时热更新
2. 环境变量（如 `DH_REPORTER_ENDPOINT`）
3. `~/.deepharness/config.yaml` 配置文件

### 3.2 数据库 Schema（dh-db）

```sql
-- 失败重试队列（含死信）
CREATE TABLE IF NOT EXISTS reporter_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    audit_log_id INTEGER NOT NULL,
    payload BLOB NOT NULL,              -- 序列化后的 OTLP LogRecord (protobuf bytes)
    failures INTEGER DEFAULT 0,         -- 已失败次数
    status TEXT DEFAULT 'pending',      -- pending / dead
    created_at TEXT NOT NULL,           -- ISO 8601
    next_retry_at TEXT NOT NULL,        -- 下次重试时间（指数退避）
    FOREIGN KEY (audit_log_id) REFERENCES audit_logs(id)
);

-- 上报游标（追踪已同步的 audit_log id）
CREATE TABLE IF NOT EXISTS reporter_cursor (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- 初始值: INSERT INTO reporter_cursor (key, value) VALUES ('last_sync_id', '0');
```

### 3.3 OTLP LogRecord 映射

| audit_logs 字段 | OTLP LogRecord 位置 |
|-----------------|---------------------|
| `id` | attribute: `audit.log_id` |
| `request_id` | attribute: `audit.request_id` |
| `session_id` | attribute: `session.id` |
| `model` | attribute: `llm.model` |
| `tokens_in` | attribute: `llm.tokens.in` (i64) |
| `tokens_out` | attribute: `llm.tokens.out` (i64) |
| `latency_ms` | attribute: `llm.latency_ms` (i64) |
| `rule_triggered` | attribute: `policy.rule_triggered` |
| `content` | body: StringValue（或哈希后字符串） |
| `response` | attribute: `llm.response`（或哈希后字符串） |
| `created_at` | `time_unix_nano` (timestamp) |

**Resource 属性（固定）：**
- `service.name` = `deepharness-gatewayd`
- `service.version` = crate version
- `host.name` = hostname

---

## 4. 组件详细设计

### 4.1 Reporter 启动流程（`reporter/mod.rs`）

```rust
pub async fn start(
    db: Arc<SqlitePool>,
    config: ReporterConfig,
) -> Result<ReporterHandle> {
    // 1. 确保表存在
    dh_db::migration::ensure_reporter_tables(&db).await?;

    // 2. 读取游标
    let last_sync_id = load_cursor(&db).await?;

    // 3. 创建自定义 Exporter
    let exporter = AuditLogExporter::new(db.clone(), config.clone());

    // 4. 配置 BatchLogProcessor
    let processor = BatchLogProcessor::builder(exporter)
        .with_batch_size(config.batch_size)
        .with_scheduled_delay(Duration::from_secs(config.flush_interval_secs))
        .build();

    // 5. 启动 Poller
    let poller = spawn_poller(db.clone(), processor, last_sync_id, config.clone());

    // 6. 启动 Retry Worker
    let retry_worker = spawn_retry_worker(db, config);

    Ok(ReporterHandle { poller, retry_worker })
}
```

### 4.2 Poller（`reporter/poller.rs`）

```rust
async fn poller_loop(
    db: Arc<SqlitePool>,
    processor: BatchLogProcessor<AuditLogExporter>,
    mut last_sync_id: i64,
    interval: Duration,
) {
    loop {
        let logs = fetch_audit_logs_after(&db, last_sync_id, 100).await;
        if logs.is_empty() {
            sleep(interval).await;
            continue;
        }

        for log in logs {
            let record = transform(log);
            processor.emit(record);
            last_sync_id = log.id;
        }

        save_cursor(&db, last_sync_id).await;
    }
}
```

**查询语句：**
```sql
SELECT * FROM audit_logs
WHERE id > ?
ORDER BY id
LIMIT ?
```

### 4.3 Exporter（`reporter/exporter.rs`）

实现 `opentelemetry_sdk::logs::LogBatchExporter` trait：

```rust
#[async_trait]
impl LogBatchExporter for AuditLogExporter {
    async fn export(&mut self, batch: Vec<LogData>) -> LogResult<()> {
        let request = build_otlp_request(batch)?;

        let mut req = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/x-protobuf")
            .body(request.encode_to_vec());

        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) if resp.status().is_client_error() => {
                // 4xx: 标记 dead letter，不再重试
                self.mark_dead_letters(&batch, resp.status()).await?;
                Ok(())
            }
            Ok(resp) => {
                // 5xx: 写入队列，由 retry worker 重试
                self.enqueue_for_retry(batch).await?;
                Err(LogError::Other(format!("HTTP {}", resp.status())))
            }
            Err(e) => {
                // 网络错误: 写入队列
                self.enqueue_for_retry(batch).await?;
                Err(LogError::Other(e.to_string()))
            }
        }
    }
}
```

### 4.4 Retry Worker（`reporter/queue.rs`）

```rust
async fn retry_worker_loop(db: Arc<SqlitePool>, config: ReporterConfig) {
    loop {
        let pending = fetch_due_queue_items(&db, Utc::now(), 50).await;

        for item in pending {
            match retry_single(&config, &item.payload).await {
                Ok(_) => delete_queue_item(&db, item.id).await,
                Err(_) => {
                    let failures = item.failures + 1;
                    if failures >= config.max_retries {
                        update_queue_item_dead(&db, item.id, failures).await;
                    } else {
                        let next_retry = calc_backoff(failures);
                        update_queue_item(&db, item.id, failures, next_retry).await;
                    }
                }
            }
        }

        sleep(Duration::from_secs(10)).await;
    }
}
```

**退避公式：**
```
delay = min(10 * 2^failures, 3600) 秒
```

### 4.5 脱敏逻辑（`reporter/transform.rs`）

```rust
fn sanitize_body(content: &str) -> String {
    if content.len() > 64 {
        let hash = sha256(content);
        format!("{}...{}...{}",
            &content[..16],
            &hash[..8],
            &content[content.len()-8..]
        )
    } else {
        sha256(content)
    }
}
```

当 `sanitize_content = true` 时：
- `content` → 脱敏后字符串
- `response` → 脱敏后字符串
- 本地 `audit_logs` 表**永远保留原始明文**

---

## 5. Admin API 扩展

### 5.1 配置管理

```
PUT /admin/config/reporter
Content-Type: application/json

{
    "enabled": true,
    "endpoint": "http://otel-collector:4318/v1/logs",
    "api_key": "sk-xxx",
    "batch_size": 100,
    "flush_interval_secs": 30,
    "sanitize_content": true,
    "max_retries": 10
}
```

```
GET /admin/config/reporter
→ 返回当前配置（api_key 字段脱敏显示为 ***）
```

### 5.2 状态查询

```
GET /admin/reporter/status
→ {
    "enabled": true,
    "endpoint": "http://otel-collector:4318/v1/logs",
    "last_sync_id": 15234,
    "queue_depth": 3,
    "last_export_at": "2026-06-10T10:51:00Z",
    "records_exported_total": 15231
}

GET /admin/reporter/queue?page=1&limit=20
→ 返回 reporter_queue 中的待重试项（分页）

GET /admin/reporter/dead-letters?page=1&limit=20
→ 返回 reporter_queue 中 status='dead' 的死信记录（分页）
```

---

## 6. 错误处理

| 错误场景 | 处理策略 | 说明 |
|---------|----------|------|
| OTLP 端点不可达（DNS/连接失败） | 写入 SQLite 队列，指数退避重试 | 不阻塞本地审计 |
| HTTP 4xx（认证失败、格式错误） | 标记 dead letter，不再重试 | 4xx 不会自愈 |
| HTTP 5xx | 写入队列，正常退避重试 | 服务端临时故障 |
| OTLP PartialSuccess | 对失败记录单独重试 | OTLP 协议特性 |
| SQLite 队列写入失败 | 打印 error log，数据可能丢失 | 依赖磁盘监控 |
| 配置热更新时 endpoint 变更 | 新配置对下次 export 生效 | 不中断正在进行的 batch |
| Gatewayd 崩溃 | 重启后从 last_sync_id 恢复 | 持久化保证 at-least-once |

---

## 7. 测试策略

### 7.1 单元测试

| 目标 | 方法 | 验证点 |
|------|------|--------|
| `transform.rs` | mock AuditLog 输入 | LogRecord 属性映射正确 |
| `sanitize` | 不同长度 content | 哈希格式正确 |
| `exporter.rs` | mock reqwest + wiremock | HTTP 请求 Header/Body 正确 |
| `retry_backoff` | 直接调用 | 指数退避公式正确 |
| `queue.rs` | 内存 SQLite (`#sqlx::test`) | CRUD 逻辑正确 |

### 7.2 集成测试

- **OTLP 兼容性测试**：mock OTLP collector，验证完整上报链路
- **断网恢复测试**：模拟 5xx → 验证入队 → 恢复 200 → 验证重试成功 → 验证队列清空

### 7.3 手动 E2E

- 配置本地 Jaeger/Tempo
- 运行 `deepharness exec opencode` 发送 LLM 请求
- 在 observability 平台验证日志可见

---

## 8. 依赖清单

```toml
# apps/gatewayd/Cargo.toml
[dependencies]
# 已有依赖：tokio, serde, reqwest, sqlx, ...

# OpenTelemetry
opentelemetry = "0.24"
opentelemetry_sdk = { version = "0.24", features = ["logs", "rt-tokio"] }
opentelemetry-otlp = { version = "0.17", features = ["logs", "http-proto"] }

# Protobuf 编码（用于 ExportLogsServiceRequest）
prost = "0.13"

# 哈希脱敏
sha2 = "0.10"
hex = "0.4"
```

---

## 9. 验收标准

1. [ ] 配置 `enabled=false` 时，reporter 模块不启动，audit.rs 不受影响
2. [ ] 配置 `enabled=true` + 正确 endpoint 时，audit_logs 数据通过 OTLP/HTTP 成功上报
3. [ ] 断开网络时，数据写入 `reporter_queue`，不丢失
4. [ ] 网络恢复后，`retry_worker` 自动重试并清空队列
5. [ ] HTTP 4xx 错误时，记录标记为 dead letter，不再无限重试
6. [ ] `sanitize_content=true` 时，上报的 content/response 为哈希值，本地仍为明文
7. [ ] Admin API 支持实时查询 reporter 状态和队列深度
8. [ ] `cargo test` 中 reporter 相关单元测试全部通过
9. [ ] `cargo check` 0 warnings

---

## 10. 未来扩展点

- **Metrics/Traces**: 基于同一套 opentelemetry_sdk 基础设施，低成本扩展
- **压缩**: OTLP HTTP Body 启用 gzip 压缩（降低带宽）
- **采样率**: 支持按百分比采样上报（如只上报 10% 的 audit_logs）
- **多端点**: 同时上报至多个 OTLP collector（主备）
