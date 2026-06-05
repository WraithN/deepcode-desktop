# 2026-06-06: tokio::spawn panic in Tauri setup closure

## 现象

运行 `bash run-desktop.sh` 启动应用时，进程立即崩溃：

```
thread 'main' panicked at src/main.rs:829:13:
there is no reactor running, must be called from the context of a Tokio 1.x runtime
```

## 根因

在 `main.rs` 的 `tauri::Builder::setup` 闭包中使用了 `tokio::spawn` 启动 SSE 监听器后台任务：

```rust
let svc_for_sse = opencode_service.clone();
tokio::spawn(async move {
    svc_for_sse.start_event_listener().await;
});
```

Tauri 的 `setup` 闭包运行在同步上下文中，不在 Tokio runtime 内。`tokio::spawn` 必须在 Tokio runtime 的上下文中调用，否则会 panic。

## 解决方案

将 `tokio::spawn` 替换为 `tauri::async_runtime::spawn`，这是 Tauri 提供的异步运行时，可以在 setup 闭包中安全使用：

```rust
let svc_for_sse = opencode_service.clone();
tauri::async_runtime::spawn(async move {
    svc_for_sse.start_event_listener().await;
});
```

## 验证结果

- `cargo build --release` 编译通过
- `bash run-desktop.sh` 启动成功，进程正常运行（PID 确认）
