# 缺陷：Claude Plugin 进程重启死锁与事件流中断

## 现象

当 claude-code 子进程意外退出后，gatewayd 日志显示 `"existing transport is dead, restarting"`，但后续 **永远不出现** `"starting Claude process..."` 日志，且通过 debug 文件 (`/tmp/claude_code_output*.txt`) 确认 reader 线程已退出。

之后客户端无论发送多少条消息，`processRawEvent` 等事件流回调不再触发。需要手动重启 gatewayd 才能恢复。

## 根因

两个独立 bug 叠加导致：

### Bug 1：Transport Mutex 死锁（`ensure_started` 中）

`crates/claude-plugin/src/instance.rs:143`：
```rust
if let Some(handle) = self.transport_guard().await.as_mut() {
    // ...
    *self.transport_guard().await = None;  // ← 死锁
```

Rust 的临时作用域规则导致 `transport_guard()` 返回的 `TokioMutexGuard` 存活到 `if` 块结束。在 `if` 块内再次调用 `transport_guard()` 会尝试重新获取同一个锁 → **死锁**。

### Bug 2：Reader 线程未重生（`out_rx` 被消费未重置）

```rust
if let Some(out_rx) = self.out_rx.lock().unwrap().take() {
    self.spawn_reader(out_rx, ...);
}
```

首次启动时 `out_rx` 字段有值，`.take()` 将其取出并传给 `spawn_reader`。进程死亡后第二次进入 `ensure_started` 时，`out_rx` 为 `None`，reader 不再启动。新进程的 stdout 无人消费，`do_send` 向无接收者的 channel 发送消息，消息全部丢失。

## 解决方案

### Fix 1：同一 guard 变量，消除重入

```rust
let mut transport_guard = self.transport_guard().await;
if let Some(handle) = transport_guard.as_mut() {
    if handle.is_alive() { return Ok(()); }
    *transport_guard = None;  // 通过同一 guard 赋值
    // ...
}
drop(transport_guard);
```

获取一次 guard 后通过 `as_mut()` 检查存活状态，使用同一 guard 设置为 `None`，避免重复获取锁。

### Fix 2：重启时创建新 channel + 新 reader

```rust
let (new_out_tx, new_out_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
*self.out_tx.lock().unwrap() = Some(new_out_tx);
self.spawn_reader(new_out_rx, self.transport.clone(), self.shutdown.clone());
```

每次进程死后重启都创建全新的 channel，存储新 `out_tx` 供 `do_send` 使用，同时将 `new_out_rx` 传入新 reader 任务。

## 文件变更

- `crates/claude-plugin/src/instance.rs`：两项修复 + 删除不再使用的 `out_rx` 字段
