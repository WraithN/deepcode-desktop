# 2026-07-24 - RUN_FINISHED 先于 agent 流式事件到达，前端事件顺序错乱

## 现象

前端向 gatewayd 发起一次 run 后，**先收到 `RUN_FINISHED`，后收到 claude code 真正产生的 `TEXT_MESSAGE_*` / `TOOL_CALL_*` 事件**。`RUN_FINISHED` 提前到达导致前端结束标志置位，后续流式内容无法正确呈现，事件顺序错乱。

链路时序（修复前）：

1. `start_run` 广播 `RUN_STARTED` + `STATE_SNAPSHOT`（session.rs:615-631）
2. `agent_service.send_message()` → `ClaudeInstance::send_message` 只 await `ensure_started` + `do_send`，消息写入 mpsc 通道即返回 Ok（instance.rs:466-485），**不等 claude 子进程处理完成**
3. `start_run` 收到 Ok 后立刻广播 `RUN_FINISHED`（session.rs:665）
4. 与此同时 claude code 子进程仍在 stream-json 模式下处理，其事件经 `AguiEventSink` 异步广播，全部晚于 `RUN_FINISHED`

## 根因

`ClaudeInstance::send_message` 的语义是"消息入队即返回"（fire-and-forget），而 `SessionManager::start_run` 错误地把"消息已写入 agent 进程"当成"回合已结束"，在 `send_message` 返回后立即抢发 `RUN_FINISHED`。

设计文档 `docs/superpowers/specs/2026-06-26-ag-ui-gatewayd-design.md`（第 111、256 行）约定的正确语义是：**`agent.done` → `TextMessageEnd` + `RUN_FINISHED`**，即终态事件必须由回合真正结束的信号驱动，当前实现偏离了设计。

排查中同时发现的关联问题：

1. **`message_stop` 被映射为 `ProcessEvent::Done`**（claude-plugin parser.rs:342）：claude 在工具调用循环中每条 assistant 消息结束都会发 `message_stop`，一个回合内出现多次。真正的回合结束信号是 `result` 事件。若直接用现有 `agent.done` 驱动 `RUN_FINISHED`，回合会在第一次工具调用前被错误结束。桌面端同样受影响：前端 `useWebSocketListeners.ts` 用 `agent.done` 置 `is_complete` 并停止流式状态，回合中途就会提前"完成"。
2. **`run_active` 保护失效**：`start_run` 中 `run_active` 仅在 `send_message` 入队的几毫秒内为 true，reaper 的长时间运行保护（`is_expired`）实际从未覆盖真正的回合执行期。
3. **进程中途死亡无终态**：claude 子进程在回合执行中途死亡时没有任何 `done`/`error` 事件，若终态改由 `agent.done` 驱动，前端会永远等待。

## 解决方案

核心思路：`RUN_FINISHED` 从 `start_run` 的抢发改为由事件流水线在 `agent.done`（回合真正结束）时补发，与内容事件走同一条串行广播通道，顺序严格保证。

### 1. 区分"消息结束"与"回合结束"（agent-core + claude-plugin）

- `agent-core` 新增 `ProcessEvent::MessageEnd`（映射为 `agent.message_end`），语义为"一条 assistant 消息结束"；`ProcessEvent::Done`（`agent.done`）语义收紧为"整个回合结束"
- claude parser：`message_stop` → `MessageEnd`，`result` → `Done`。opencode（`session.idle`）与 codex（`turn/completed`）的 `Done` 本来就是真回合结束，无需改动
- 桌面端副作用为正向：`agent.done` 现在只在回合真正结束时到达，`is_complete` 与流式状态不再被中途误置

### 2. RUN_FINISHED 由 sink 消费者补发（gatewayd）

- `Session` 新增 `current_run_id` 跟踪，`begin_run` / `end_run` 管理 run 生命周期（同时维护 `run_active`，使 reaper 保护覆盖整个回合）；`start_run` 拒绝并发 run（`RunError::RunAlreadyActive`，AG-UI 同一 thread 同一时间只允许一个 run）
- `start_run` 不再抢发 `RUN_FINISHED`；`send_message` 失败时 `end_run` 回滚登记
- `AguiEventSink` 消费者在广播完 `agent.done` 的本批事件（含 `TextMessageEnd`）后补发 `RUN_FINISHED`（携带登记的 run_id）；`agent.error` 时复位 run 登记（`RUN_ERROR` 已由 mapper 产出）
- 兜底：`done`/`error` 即使 mapper 产出为空（如本轮无任何文本）也必须送达消费者，否则 run 永远悬挂
- SSE handler 对 `RunAlreadyActive` 只记日志不广播 `RUN_ERROR`，避免污染正在执行的 run 的事件流

### 3. claude 进程中途死亡补发错误（claude-plugin）

- `ClaudeInstance` 新增 `turn_active` 在途跟踪：`send_message` 写入成功后置位，收到 `Done`/`Error` 复位（`MessageEnd` 不复位）
- reader 检测到进程死亡且回合在途时，补发 `ProcessEvent::Error` → `RUN_ERROR` 终止回合，避免前端干等

### 已知限制

- claude 进程存活但长时间无响应（挂起）时，run 会一直处于在途状态，session 不会被空闲回收。该场景需要运行级超时看门狗，超出本次修复范围。

### 修改文件

- `crates/agent-core/src/process/event.rs` — 新增 `ProcessEvent::MessageEnd`
- `crates/agent-core/src/process/mapper.rs` — `MessageEnd` → `agent.message_end` 映射 + 测试
- `crates/claude-plugin/src/parser.rs` — `message_stop` → `MessageEnd` + 回归测试
- `crates/claude-plugin/src/constants.rs` — 新增 `ERR_PROCESS_DIED`
- `crates/claude-plugin/src/instance.rs` — `turn_active` 跟踪；进程死亡补发错误；提取 `emit_to_frontend`
- `apps/gatewayd/src/session.rs` — `current_run_id` 跟踪、`begin_run`/`end_run`、`RunAlreadyActive`、移除抢发 `RUN_FINISHED` + 单元测试
- `apps/gatewayd/src/agui/mapper.rs` — `agent.message_end` 走原 `map_done` 逻辑（保持文本分段渲染不变）
- `apps/gatewayd/src/agui_sink.rs` — 消费者补发 `RUN_FINISHED` / 错误时复位 run；终态空事件不丢弃 + 集成测试
- `apps/gatewayd/src/handlers/sse.rs` — `RunAlreadyActive` 只记日志

### 验证

1. `cargo test -p agent-core -p claude-plugin -p dh-gatewayd` — 62 tests passed（含新增回归测试：`RUN_FINISHED` 排在 `TEXT_MESSAGE_END` 之后、`message_end` 不结束 run、游离 `done` 不产生 `RUN_FINISHED`、`begin_run` 拒绝并发 run）
2. `cargo check --workspace` 与 `src-tauri` — 0 warnings
