# OpenCode Plugin SSE 按需重连实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `crates/opencode-plugin` 的 SSE 空闲超时问题：静默超时、按需重建 SSE；同时拆分 `instance.rs` 为 `instance.rs` + `sse.rs`。

**Architecture:** 为 SSE 单独配置较长 timeout 的 `reqwest::Client`；在 `OpencodeInstance` 中维护 `sse_running` 和 `sse_handle` 状态；SSE loop 检测到超时/连接重置时静默退出并设置 `sse_running=false`；`send_message` / `respond` 在发送前检查并重建 SSE loop。

**Tech Stack:** Rust, tokio, reqwest, futures-util

---

## 文件变更清单

- **Modify:** `crates/opencode-plugin/src/instance.rs`
  - 删除 SSE loop / forward 函数，改为引用 `sse.rs`
  - 新增 `sse_client`、SSE 状态字段
  - `ensure_started()` 不再启动 SSE loop
  - 新增 `ensure_sse_connected()`
  - `send_message()` / `respond()` 调用 `ensure_sse_connected()`
  - `stop()` 增加 SSE task 清理
- **Create:** `crates/opencode-plugin/src/sse.rs`
  - `SseEvent` 结构体
  - `sse_event_loop()` —— 静默退出版
  - `forward_sse_to_event_sink()`
  - `is_sse_disconnect_error()`
- **Modify:** `crates/opencode-plugin/src/lib.rs`
  - 新增 `pub mod sse;`

---

## 说明：Mutex 替换策略

`AgentInstance::status()` 和 `AgentInstance::endpoint()` 是 sync trait 方法，因此 `status` 和 `base_url` 不能简单替换为 `tokio::sync::Mutex`（否则 sync 方法中必须调用 `blocking_lock()`，可能 panic）。

本次只将**仅在 async 路径中使用**的 `event_sender` 改为 `tokio::sync::Mutex`；`status`、`base_url`、`sessions` 保持 `std::sync::Mutex`。

---

### Task 1: 创建 `sse.rs` 并提取 SSE 相关代码

**Files:**
- Create: `crates/opencode-plugin/src/sse.rs`
- Modify: `crates/opencode-plugin/src/lib.rs`

- [ ] **Step 1: 创建 `sse.rs`，包含 `SseEvent` 和错误分类 helper**

```rust
use agent_core::event_sink::DynEventSink;
use agent_core::logger::SessionLogger;
use futures_util::StreamExt;
use serde_json::json;
use std::sync::Arc;

/// SSE event as emitted by `opencode serve`.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: String,
    pub session_id: Option<String>,
    pub payload: serde_json::Value,
}

/// Classify reqwest errors that are normal SSE disconnects (timeout, reset, EOF).
pub fn is_sse_disconnect_error(e: &reqwest::Error) -> bool {
    e.is_timeout()
        || e.is_connect()
        || e.is_request()
        || e.to_string().contains("connection reset")
        || e.to_string().contains("Connection reset")
        || e.to_string().contains("broken pipe")
        || e.to_string().contains("Broken pipe")
        || e.to_string().contains("Unexpected EOF")
}
```

- [ ] **Step 2: 将 `sse_event_loop` 和 `forward_sse_to_event_sink` 从 `instance.rs` 原样移动到 `sse.rs`**

移动后调整函数签名：

```rust
pub async fn sse_event_loop(
    base_url: &str,
    client: reqwest::Client,
    sender: tokio::sync::broadcast::Sender<SseEvent>,
    event_sink: DynEventSink,
    instance_id: &str,
    logger: Arc<SessionLogger>,
    sse_running: Arc<std::sync::atomic::AtomicBool>,
)
```

Loop 体改成**单次连接、出错静默退出、不再重连**：

```rust
{
    let url = format!("{}/event", base_url);

    match client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await
    {
        Ok(resp) => {
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(data)
                                {
                                    let event_type = payload
                                        .get("type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    let session_id = payload
                                        .get("properties")
                                        .and_then(|p| p.get("sessionID"))
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    let _ = sender.send(SseEvent {
                                        event_type: event_type.clone(),
                                        session_id: session_id.clone(),
                                        payload: payload.clone(),
                                    });

                                    forward_sse_to_event_sink(
                                        &event_type,
                                        &payload,
                                        &event_sink,
                                        instance_id,
                                        &logger,
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if is_sse_disconnect_error(&e) {
                            log::info!(
                                "[opencode SSE] disconnected ({e}), will reconnect on next message"
                            );
                        } else {
                            log::warn!("[opencode SSE] stream error: {}", e);
                        }
                        break;
                    }
                }
            }
        }
        Err(e) => {
            if is_sse_disconnect_error(&e) {
                log::info!(
                    "[opencode SSE] connect failed ({e}), will reconnect on next message"
                );
            } else {
                log::warn!("[opencode SSE] connect error: {e}, will reconnect on next message");
            }
        }
    }

    sse_running.store(false, std::sync::atomic::Ordering::SeqCst);
    log::info!("[opencode SSE] listener stopped");
}
```

`forward_sse_to_event_sink` 从 `instance.rs` 原样移动，不需要改动。

- [ ] **Step 3: 在 `lib.rs` 中导出 sse 模块**

```rust
pub mod instance;
pub mod mapper;
pub mod mcp_adapter;
pub mod parser;
pub mod plugin;
pub mod sse;
```

- [ ] **Step 4: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS（`instance.rs` 还未引用 `sse.rs`，所以暂时不会有链接错误；此步骤确认 `sse.rs` 自身可编译）

---

### Task 2: 修改 `instance.rs` 字段与初始化

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:1-51`

- [ ] **Step 1: 调整 imports**

```rust
use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus};
use agent_core::logger::{LogLevel, SessionLogger};
use crate::sse::{is_sse_disconnect_error, SseEvent};
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
```

删除 `use futures_util::StreamExt;`。

- [ ] **Step 2: 修改 `OpencodeInstance` 结构体**

```rust
pub struct OpencodeInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    client: reqwest::Client,
    sse_client: reqwest::Client,
    base_url: Mutex<Option<String>>,
    serve_process: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    status: Arc<Mutex<InstanceStatus>>,
    event_sender: Arc<tokio::sync::Mutex<Option<tokio::sync::broadcast::Sender<SseEvent>>>>,
    started: Arc<std::sync::atomic::AtomicBool>,
    /// conversation_id -> opencode_session_id
    sessions: Arc<Mutex<HashMap<String, String>>>,
    sse_running: Arc<std::sync::atomic::AtomicBool>,
    sse_handle: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}
```

- [ ] **Step 3: 修改 `new()` 初始化**

```rust
pub fn new(config: InstanceConfig, event_sink: DynEventSink, logger: Arc<SessionLogger>) -> Self {
    Self {
        config,
        event_sink,
        logger,
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
        sse_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
        base_url: Mutex::new(None),
        serve_process: Arc::new(tokio::sync::Mutex::new(None)),
        status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
        event_sender: Arc::new(tokio::sync::Mutex::new(None)),
        started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        sse_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        sse_handle: Arc::new(tokio::sync::Mutex::new(None)),
    }
}
```

- [ ] **Step 4: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS（可能有未使用字段 warning，下一步消除）

---

### Task 3: 拆分 SSE 启动逻辑

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:67-174`

- [ ] **Step 1: 从 `ensure_started()` 中移除 SSE 启动代码**

删除以下代码块：

```rust
// Setup broadcast channel for SSE events
let (tx, _rx) = tokio::sync::broadcast::channel::<SseEvent>(1000);
{
    let mut guard = self.event_sender.lock().unwrap();
    *guard = Some(tx.clone());
}

// Spawn SSE listener
let base_url_for_sse = base_url.clone();
let client_for_sse = self.client.clone();
let event_sink_for_sse = self.event_sink.clone();
let instance_id = self.config.id.clone();
let logger = self.logger.clone();
tokio::spawn(async move {
    sse_event_loop(
        &base_url_for_sse,
        client_for_sse,
        tx,
        event_sink_for_sse,
        &instance_id,
        logger,
    )
    .await;
});
```

注意：`event_sender` 已改为 `tokio::sync::Mutex`，所以原有 `self.event_sender.lock().unwrap()` 在 `ensure_started` 中也需要删除。

- [ ] **Step 2: 新增 `ensure_sse_connected()` 方法**

放在 `ensure_started()` 之后、`create_opencode_session()` 之前：

```rust
/// Start the SSE listener if not already running.
async fn ensure_sse_connected(&self) -> Result<(), InstanceError> {
    if self
        .sse_running
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return Ok(());
    }

    let base_url = self.base_url().ok_or_else(|| {
        InstanceError::NotRunning("opencode serve not started".into())
    })?;

    let (tx, _rx) = tokio::sync::broadcast::channel::<SseEvent>(1000);
    {
        let mut guard = self.event_sender.lock().await;
        *guard = Some(tx.clone());
    }

    let sse_client = self.sse_client.clone();
    let event_sink_for_sse = self.event_sink.clone();
    let instance_id = self.config.id.clone();
    let logger = self.logger.clone();
    let sse_running = self.sse_running.clone();
    let handle = tokio::spawn(async move {
        crate::sse::sse_event_loop(
            &base_url,
            sse_client,
            tx,
            event_sink_for_sse,
            &instance_id,
            logger,
            sse_running,
        )
        .await;
    });

    {
        let mut guard = self.sse_handle.lock().await;
        *guard = Some(handle);
    }

    // Give the connection a moment to establish so early events are not lost.
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    Ok(())
}
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 4: 在 `send_message` / `respond` 中按需重建 SSE

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:254-321`

- [ ] **Step 1: 修改 `send_message`**

在 `self.ensure_started().await?;` 之后、创建 session 之前插入：

```rust
self.ensure_started().await?;
self.ensure_sse_connected().await?;
```

- [ ] **Step 2: 修改 `respond`**

同样在 `self.ensure_started().await?;` 之后插入：

```rust
self.ensure_started().await?;
self.ensure_sse_connected().await?;
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 5: 更新 `stop()` 清理 SSE task

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:323-335`

- [ ] **Step 1: 在 `stop()` 中 abort SSE handle 并重置状态**

```rust
fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
    Box::pin(async move {
        if let Some(handle) = self.sse_handle.lock().await.take() {
            handle.abort();
        }
        self.sse_running.store(false, std::sync::atomic::Ordering::SeqCst);

        if let Some(mut child) = self.serve_process.lock().await.take() {
            let _ = child.start_kill();
        }
        {
            let mut guard = self.status.lock().unwrap();
            *guard = InstanceStatus::Stopped;
        }
        self.emit_status(InstanceStatus::Stopped);
        Ok(())
    })
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 6: 删除 `instance.rs` 中已移动到 `sse.rs` 的残留代码

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:338-492`

- [ ] **Step 1: 删除 `sse_event_loop` 和 `forward_sse_to_event_sink` 函数**

删除第 338–492 行（SSE event loop 和 forward 函数）。

- [ ] **Step 2: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 7: 编译检查与 warning 清零

**Files:**
- 无需修改文件（修复编译 warning）

- [ ] **Step 1: 运行 crate 检查**

Run: `cargo check -p opencode-plugin`
Expected: 0 errors, 0 warnings

- [ ] **Step 2: 运行 Tauri lib 检查**

Run: `cargo check --lib -p dh-desktop`
Expected: 0 errors, 0 warnings

- [ ] **Step 3: 运行 workspace 检查**

Run: `cargo check --workspace`
Expected: 0 errors, 0 warnings

- [ ] **Step 4: 运行 clippy（如环境允许）**

Run: `cargo clippy -p opencode-plugin -- -D warnings`
Expected: 如果 rustc 1.95 ICE 未触发，则 PASS

---

### Task 8: 端到端验证

**Files:**
- 无需修改文件

- [ ] **Step 1: 启动 gatewayd 并 attach opencode**

Run: `dh gwd start --daemon --attach opencode`
Expected: gatewayd 启动成功

- [ ] **Step 2: 等待 3 分钟以上不发送消息**

Expected: 日志中**不应出现** `[opencode SSE] stream error: error decoding response body`
可以出现 `[opencode SSE] listener stopped` 或 `[opencode SSE] disconnected ... will reconnect on next message`（info 级别）

- [ ] **Step 3: 发送第一条消息**

Run: `dh chat --interactive opencode`
输入: `你好`
Expected:
- `[status]>>>> running`
- 收到 `[ai]>>>> 你好！...` 回复

- [ ] **Step 4: 再次等待 3 分钟以上**

Expected: 无 ERROR 级 SSE 日志

- [ ] **Step 5: 发送第二条消息**

Expected: SSE 自动重建，正常收到 AI 回复

- [ ] **Step 6: 停止 gatewayd**

Run: `dh gwd stop`
Expected: 进程正常退出，无残留 SSE task panic

---

## Self-Review Checklist

- [x] **Spec coverage:** 所有 spec 要求都有对应 task
  - SSE 专用 client → Task 2
  - SSE 状态管理 → Task 2 / Task 3
  - SSE loop 静默退出 → Task 1
  - 消息发送前重建 SSE → Task 4
  - stop() 清理 → Task 5
  - 拆分 instance.rs → Task 1 + Task 6
- [x] **Placeholder scan:** 无 TBD / TODO / "implement later"
- [x] **Type consistency:** `SseEvent`、`sse_event_loop`、`is_sse_disconnect_error` 在 `sse.rs` 中定义并导出；`instance.rs` 通过 `crate::sse::*` 引用
- [x] **Mutex 策略说明:** 明确解释为何只改 `event_sender`，保持 `status`/`base_url`/`sessions`
