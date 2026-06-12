# OpenCode SSE 按需重连实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 改造 `crates/opencode-plugin/src/instance.rs`，让 opencode SSE 事件流在空闲超时后静默断开，待下次 `send_message` / `respond` 时自动重建。

**Architecture:** 为 SSE 单独配置无 timeout 的 `reqwest::Client`；在 `OpencodeInstance` 中维护 `sse_running` 和 `sse_handle` 状态；SSE loop 检测到超时/连接重置时静默退出并设置 `sse_running=false`；消息发送前检查状态并在需要时重建 SSE loop。

**Tech Stack:** Rust, tokio, reqwest

---

## 文件变更清单

- **Modify:** `crates/opencode-plugin/src/instance.rs`
  - 新增 SSE 专用 client 和状态字段
  - 新增 `ensure_sse_connected()` 方法
  - 改造 `sse_event_loop()` 以支持静默退出和取消
  - 更新 `send_message()` / `respond()` 以按需重建 SSE
  - 更新 `stop()` 以清理 SSE task

---

### Task 1: 新增 SSE 状态字段和专用 client

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:20-32`

- [ ] **Step 1: 在 `OpencodeInstance` 中新增字段**

修改 `OpencodeInstance` 结构体，新增 SSE 专用 client 和状态字段：

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
    event_sender: Arc<Mutex<Option<tokio::sync::broadcast::Sender<SseEvent>>>>,
    started: Arc<std::sync::atomic::AtomicBool>,
    /// conversation_id -> opencode_session_id
    sessions: Arc<Mutex<HashMap<String, String>>>,
    sse_running: Arc<std::sync::atomic::AtomicBool>,
    sse_handle: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}
```

- [ ] **Step 2: 在 `new()` 中初始化新字段**

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
            .timeout(None)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
        base_url: Mutex::new(None),
        serve_process: Arc::new(tokio::sync::Mutex::new(None)),
        status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
        event_sender: Arc::new(Mutex::new(None)),
        started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        sse_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        sse_handle: Arc::new(tokio::sync::Mutex::new(None)),
    }
}
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS（可能还有未使用字段 warning，后续任务消除）

---

### Task 2: 实现按需启动 SSE loop 的方法

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:67-174`

- [ ] **Step 1: 提取 SSE loop 启动逻辑到 `ensure_sse_connected()`**

在 `OpencodeInstance` 中新增方法（放在 `ensure_started` 之后）：

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

    // Setup broadcast channel for SSE events
    let (tx, _rx) = tokio::sync::broadcast::channel::<SseEvent>(1000);
    {
        let mut guard = self.event_sender.lock().unwrap();
        *guard = Some(tx.clone());
    }

    let sse_client = self.sse_client.clone();
    let event_sink_for_sse = self.event_sink.clone();
    let instance_id = self.config.id.clone();
    let logger = self.logger.clone();
    let sse_running = self.sse_running.clone();
    let handle = tokio::spawn(async move {
        sse_event_loop(
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

- [ ] **Step 2: 从 `ensure_started()` 中移除 SSE loop 启动代码**

移除 `ensure_started()` 中以下代码块：

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

保留 `base_url` 设置和状态更新逻辑。

- [ ] **Step 3: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS（可能有未使用 import / 变量 warning，下一步处理）

---

### Task 3: 改造 SSE event loop 支持静默退出

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:342-412`

- [ ] **Step 1: 更新 `sse_event_loop` 签名**

```rust
async fn sse_event_loop(
    base_url: &str,
    client: reqwest::Client,
    sender: tokio::sync::broadcast::Sender<SseEvent>,
    event_sink: DynEventSink,
    instance_id: &str,
    logger: Arc<SessionLogger>,
    sse_running: Arc<std::sync::atomic::AtomicBool>,
)
```

- [ ] **Step 2: 重写 loop 体以分类错误并静默退出**

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
                            log::info!("[opencode SSE] disconnected ({e}), will reconnect on next message");
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
                log::info!("[opencode SSE] connect failed ({e}), will reconnect on next message");
            } else {
                log::warn!("[opencode SSE] connect error: {}, will reconnect on next message", e);
            }
        }
    }

    sse_running.store(false, std::sync::atomic::Ordering::SeqCst);
    log::info!("[opencode SSE] listener stopped");
}
```

- [ ] **Step 3: 新增错误分类 helper**

在文件底部 `Helpers` 区域新增：

```rust
fn is_sse_disconnect_error(e: &reqwest::Error) -> bool {
    e.is_timeout()
        || e.is_connect()
        || e.is_request()
        || e.to_string().contains("connection reset")
        || e.to_string().contains("broken pipe")
        || e.to_string().contains("Connection reset")
        || e.to_string().contains("Unexpected EOF")
}
```

- [ ] **Step 4: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 4: 在 send_message / respond 中按需重建 SSE

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

### Task 5: 更新 stop() 清理 SSE task

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:323-336`

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

### Task 6: 全量 Rust 编译检查与 warning 清零

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs`（修复任何 warning）

- [ ] **Step 1: 运行 crate 检查**

Run: `cargo check -p opencode-plugin`
Expected: 0 errors, 0 warnings

- [ ] **Step 2: 运行主 crate 检查**

Run: `cargo check --bin ai-coding-desktop`
Expected: 0 errors, 0 warnings

Run: `cargo check --lib -p ai-coding-desktop`
Expected: 0 errors, 0 warnings

- [ ] **Step 3: 运行 clippy（可选但推荐）**

Run: `cargo clippy -p opencode-plugin -- -D warnings`
Expected: PASS

---

### Task 7: 端到端验证

**Files:**
- 无需修改文件

- [ ] **Step 1: 启动 gatewayd 并 attach opencode**

Run: `dh gwd start --daemon --attach opencode`
Expected: `dh-gatewayd is already running` 或启动成功

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
  - SSE 专用 client → Task 1
  - SSE 状态管理 → Task 1 / Task 2
  - SSE loop 静默退出 → Task 3
  - 消息发送前重建 SSE → Task 4
  - stop() 清理 → Task 5
- [x] **Placeholder scan:** 无 TBD / TODO / "implement later"
- [x] **Type consistency:** `sse_running` 始终使用 `Arc<AtomicBool>`，`sse_handle` 始终使用 `Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>`
