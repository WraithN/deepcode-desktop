# 启动时报 SSE connect error: error sending request for url (http://127.0.0.1:3007/event)

## 现象

启动 Tauri 桌面应用时，日志持续输出以下错误：

```
[ERROR dh::service::opencode_service] [opencode] SSE connect error: error sending request for url (http://127.0.0.1:3007/event)
```

应用界面可以正常打开，但 OpenCode 相关功能不可用。

## 根因

`OpencodeService::new()` 在启动 `opencode serve` 子进程后，**没有验证进程是否存活、服务是否可访问**，仅 `sleep(3)` 后就直接返回 `Ok`：

```rust
let child = cmd.spawn().map_err(|e| format!("Failed to start opencode serve: {}", e))?;
std::thread::sleep(std::time::Duration::from_secs(3));
// 没有检查 child 是否已退出，也没有验证端口是否真的在监听
```

这导致以下问题：

1. **端口竞争**：`find_available_port_sync()` 使用临时 `TcpListener::bind` 检测端口可用性，释放后到 `opencode serve` 实际绑定之间，端口可能被其他实例抢占（本机 3001-3007 全被遗留的 opencode 进程占用）。
2. **启动失败不可感知**：即使 `opencode serve` 因端口被占、配置错误等原因启动失败（子进程立即退出），`OpencodeService` 仍然认为启动成功，后续 SSE listener 不断重连一个不存在的服务。
3. **错误日志风暴**：`start_event_listener` 在循环中每次重试都用 `log::error!` 输出，导致启动阶段大量 ERROR 日志。

## 解决方案

1. **在 `OpencodeService::new()` 中增加健康检查**：
   - `spawn()` 后等待并检查子进程是否仍在运行（排除进程立即崩溃的情况）。
   - 轮询尝试连接服务根地址，确认服务可访问后再返回 `Ok`。
   - 若超时（如 10 秒）仍未就绪，终止子进程并返回 `Err`，让 `main.rs` 回退到 `new_fallback()`。

2. **改进 `start_event_listener` 的日志级别**：
   - 连接失败时降级为 `warn`，避免启动阶段 ERROR 日志风暴。
   - 每次重试前打印 `info` 提示，只保留流解析错误为 `error`。

3. **扩大端口搜索范围**：将 `3001..=3010` 扩大到 `3001..=3050`，降低端口耗尽概率。
