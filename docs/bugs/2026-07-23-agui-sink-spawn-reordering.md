# 2026-07-23 - AguiEventSink emit runtime.spawn 导致 SSE 事件字节级乱序

## 现象

220 机器使用 opencode agent 时，前端 SSE 流出现 `TEXT_MESSAGE_CONTENT` 事件字节级乱序：timestamp 靠后的事件反而先到达浏览器，导致前端按到达顺序展示的答案错乱（同一个 messageId 内相邻 delta 颠倒）。

抓包数据（dh-backend 端 curl 抓 60 秒 run）：
- 共 900 个 `TEXT_MESSAGE_CONTENT` 事件
- 38 个逆序（4.2%）
- 逆序间隔 23-86 微秒，全部在同一 messageId 内（同一段回答）

## 根因

`apps/gatewayd/src/agui_sink.rs` 的 `AguiEventSink::emit` 每次调用都用 `runtime.spawn` 异步执行 broadcast：

```rust
fn emit(&self, event_type: &str, payload: Value) {
    // ... mapper.map(...) ...
    self.runtime.spawn(async move {
        let session_id = /* ... */;
        for event in events {
            session_manager.broadcast(&session_id, event).await;
        }
    });
}
```

bug 链：
1. opencode-plugin 高频连续 emit `agent.token`（每条 token 一次，间隔 30-100μs）
2. 每次 emit 独立 `runtime.spawn` 一个 tokio task
3. tokio **不保证**两个独立 spawn task 的调度顺序
4. `spawn(A)` 和 `spawn(B)` 之间发生 microtask 调度，B 的 broadcast 抢在 A 前面
5. `broadcast::Sender::send` 顺序 = receiver 收到顺序
6. dh-backend 转发给前端，前端按到达顺序渲染 → 答案错乱

**为什么直连测不出来**：之前对比测试直连 2346 端口未复现，是因为直连测试的 5 个 token 间隔较大（毫秒级），spawn 调度窗口里没抢上。高频 stream 时（每个 token 间隔 30-100μs）才暴露。

**排除项**（均非根因）：
- `AguiMapper::map` 是同步串行（一次 `map_token` 一次 `now()`）
- `AguiEventStream` unfold 单 consumer 串行 recv
- `broadcast::Sender::send` 内部有锁，receiver 收到顺序 = send 顺序

根因唯一在 `emit → broadcast` 之间的 `runtime.spawn` 异步化。

## 解决方案

采用 **方案 B：mpsc 通道串行化**。移除 per-emit `runtime.spawn`，改为：

1. `emit` 同步执行 mapper 映射（保持状态顺序），将结果封装为 `EmitJob` 投递到 `mpsc::UnboundedSender`（FIFO、同步、非阻塞）。
2. 在 `AguiEventSink::new` 时 spawn **唯一一个**常驻 consumer task（`consumer_loop`），串行消费 channel：解析 session_id → 逐条 broadcast。
3. 单 consumer + 串行 await 保证 dequeue 顺序 = enqueue 顺序 = emit 顺序 = broadcast 顺序。

选择方案 B 而非方案 A（同步 broadcast）的原因：方案 A 需要在 tokio runtime 内 `block_on` 异步的 `session_for_instance` / `get_session`，容易死锁或 panic；方案 B 保留 async 边界，mpsc 天然 FIFO，代码改动约 30 行。

### 修改文件
- `apps/gatewayd/src/agui_sink.rs` — 重写 `emit` 为 channel 投递；新增 `EmitJob` 枚举与 `consumer_loop` 单消费者 task；新增排序回归测试
- `apps/gatewayd/src/agui/mapper.rs` — 将 `METHOD_*` 常量改为 `pub` 以供测试引用（避免魔法字符串）

### 验证
1. `cargo check --lib -p dh-gatewayd` — 0 warnings
2. `cargo test --lib -p dh-gatewayd` — 21 tests passed（含新增 `test_emit_order_preserved_for_high_frequency_tokens`：连续 emit 800 个 token，断言 receiver 收到的 delta 序列与 emit 序列严格一致）
3. 修复后跑 `/home/dh/capture-many-tmc.sh`，预期逆序数从 38/900 降到 0/900，所有相邻 `TEXT_MESSAGE_CONTENT` timestamp 严格递增
