# Bug: WebSocket 未连接，发送消息报错 "WebSocket not connected"

## 发现时间
2026-06-03

## 现象
发送消息时弹出错误提示：
> 通信错误: Error: WebSocket not connected

## 根因
`src-tauri/src/main.rs` 中的 `start_ws_server` 函数创建了一个**临时的 tokio runtime**，用 `block_on` 启动 WebSocket 服务器后获取地址：

```rust
fn start_ws_server(...) -> Result<SocketAddr, String> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        ws_server.start(shutdown_rx).await
    }).map_err(|e| e.to_string())
}
```

`ws_server.start()` 内部用 `tokio::spawn` 创建了 accept 循环后台任务。但当 `block_on` 完成、函数返回后，**临时 runtime 被 `Drop`**，其中所有未完成的 spawn 任务都被取消。WebSocket 服务器实际上在应用启动后瞬间就被销毁了。

前端虽然能正常 `invoke('get_websocket_url')` 获取地址，但连接时服务器已不存在。

## 影响范围
- 前端无法与后端通信
- 所有 agent 操作（发送消息、接收事件）均失败
- 阻塞核心功能

## 解决方案
将 WebSocket 服务器放到一个**持久运行的守护线程**中，runtime 永不退出：

```rust
fn start_ws_server(
    mut ws_server: gateway::server::WebSocketServer,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<SocketAddr, String> {
    let (addr_tx, addr_rx) = std::sync::mpsc::channel::<SocketAddr>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            ws_server.start(shutdown_rx).await
        });
        match result {
            Ok(addr) => {
                addr_tx.send(addr).unwrap();
                // 保持 runtime 运行，accept 循环持续执行
                rt.block_on(std::future::pending::<()>());
            }
            Err(e) => log::error!("...", e),
        }
    });

    let addr = addr_rx.recv().map_err(|e| e.to_string())?;
    Ok(addr)
}
```

## 修复文件
- `src-tauri/src/main.rs`

## 验证方法
1. 启动 Tauri 桌面应用
2. 检查 WebSocket 是否能成功连接
3. 发送消息不应再报 "WebSocket not connected"
