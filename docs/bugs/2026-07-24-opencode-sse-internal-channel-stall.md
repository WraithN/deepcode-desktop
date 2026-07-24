# 2026-07-24 - opencode 长 run 尾部事件丢失，消息截断、预览卡片缺失

## 现象

通过 gatewayd + opencode 插件执行的长 run（如 /proto-make 原型生成），流式事件在 run 中途
（累积约 1000 条事件后）完全停止：下游 dh-backend 收到的文本在句子中间截断，最终
text part（含 `[[FILE:...]]` / `[[PROJECT:...]]` / `[[CARD:...]]` 标记）整体丢失，
前端因此无法渲染完成态内容与工程预览卡片。TEXT_MESSAGE_END / RUN_FINISHED 仍正常到达
（由 send_message HTTP 响应返回驱动），所以 run 看起来"正常结束"。

## 根因

`crates/agent-core/src/process/http.rs` 的 `forward_values` 把每条 SSE 事件同时
`send().await` 到两个 mpsc 通道：

1. **内部通道**（容量 `CHANNEL_CAPACITY = 1000`）：读取端是 `HttpHandle.receive()`。
   但 opencode 插件从不调用 `receive()`（只有 claude/codex 的 stdio 传输使用），
   该通道只进不出。累积 1000 条后 `send().await` 永久阻塞，SSE 读取任务卡死，
   opencode `/event` 流上的后续事件不再被读取、解析、转发。
2. 外部通道：由 instance relay loop 活跃消费，无问题。

代码注释声称"丢弃事件以避免阻塞 SSE reader"，但阻塞式 `send()` 并不会丢弃——
实现与设计意图相悖。

实证：某次 run 中 gatewayd relay 日志在 02:20:37.559 后再无事件，而 opencode 日志显示
deepseek 流持续到 02:20:43.69；阻塞点累积事件数（约 808 条已映射 + 未映射的
message.updated / part.updated / 心跳）恰达 1000 上限。

## 解决方案

`forward_values` 内部通道改为 `try_send`（满则丢弃），与注释意图一致；外部通道保持
`send().await` 阻塞式（relay loop 持续消费，背压可接受），保证事件不丢失。

```rust
let _ = internal_tx.try_send(value.clone());
let _ = external_tx.send(value).await;
```

### 修改文件

- `crates/agent-core/src/process/http.rs` — `forward_values` 内部通道改 `try_send` + 注释。

### 验证

1. `cargo test -p agent-core` — 24 tests passed。
2. `cargo build --release -p dh-gatewayd` — 0 warnings。
3. 端到端：经 dh-backend `POST /api/v1/agent` 重放 /proto-make 长 run（累积事件
   超过修复前 1000 条的卡死阈值），最终 text part 与 `[[CARD:...]]` 标记完整到达。
