# Agent 卡死无超时保护与重试机制缺失

## 现象

与 AI 智能体对话时，若 agent 进程卡死（如 LLM API 挂起、opencode serve 死锁、子进程假死），表现如下：

1. **后端无卡死检测**：`send_message` 的 HTTP POST 阻塞至 agent run 结束，单次请求硬超时为 1800 秒（30 分钟）。期间若 agent 既不返回也不报错，只能干等满 30 分钟。
2. **前后端超时不一致**：前端 `websocketStore.sendRequest` 硬编码 30 秒超时，30 秒后前端报 `Request timeout: agent.sendMessage` 并把 `isStreaming` 置为 false，但后端 HTTP 请求仍在挂起，造成界面状态与实际运行状态脱节。
3. **无活跃度看门狗**：现有的重试仅在 HTTP 请求**显式报错**时触发（`reset_and_restart` + 重试一次），无法覆盖"HTTP 挂起不返回也不报错"的卡死场景。

## 根因

`crates/opencode-plugin/src/instance.rs` 中 `send_message` 直接 `await` `send_message_http`，而该 HTTP 调用（`POST /session/{id}/message`）会阻塞至整个 agent run 完成：

- **缺少基于 SSE 活跃度的卡死判定**。agent 正常运行时 opencode 会持续推送 SSE 事件（thinking / tool_use / tool_result），这是天然的存活心跳；但原实现未利用该信号，仅依赖 HTTP 超时这一硬上限。
- **HTTP 超时过长**（`transport.rs` 中 `DEFAULT_TIMEOUT_SECS = 1800`），原注释说明是为放行长任务（PRD/原型生成可超 10 分钟），但同时也让真卡死场景的最长恢复时间高达 30 分钟。
- **前端超时一刀切**（`websocketStore.ts` 中所有 JSON-RPC 方法统一 30 秒），未区分短操作与 `agent.sendMessage` / `agent.respond` 这类会阻塞至 run 结束的长操作。

## 解决方案

引入**基于 SSE 事件活跃度的看门狗（watchdog）**，区分"正常长任务"与"卡死"：

1. **SSE 活跃度看门狗**（`instance.rs`）：
   - 新增 `last_event_at: Arc<Mutex<Option<Instant>>>` 字段，relay loop 每次收到非空 agent 事件即刷新时间戳。
   - `http_with_watchdog` 用 `tokio::select!` 让 HTTP 请求与 `watchdog_until_stalled` 竞争：超过 `WATCHDOG_STALL_THRESHOLD_SECS`（120s）无任何 SSE 事件即判定卡死并中断本次请求。
   - 阈值大于 agent 正常思考间隔（思考时 opencode 流式发送 thinking 事件），避免误杀正常长任务。

2. **卡死重建重试**（`send_with_watchdog_retry`）：
   - 卡死或 HTTP 失败时调用 `reset_and_restart`（杀进程 + 重启 opencode serve）并新建 session 后重试。
   - 重试上限 `MAX_SEND_RETRIES = 2`（总尝试 3 次），避免死循环重复消耗 LLM 调用。
   - `respond` 因仅有 opencode session_id 无 conversation 映射，重建后旧 session 必然失效，故卡死时只重建进程并快速失败，由前端重新触发。

3. **HTTP 超时下调**（`transport.rs`）：`DEFAULT_TIMEOUT_SECS` 由 1800 降至 1200（20 分钟），作为最终硬上限兜底；正常长任务由看门狗放行（有事件不触发），异常挂起由看门狗在 120s 内捕获。

4. **前端长操作超时**（`websocketStore.ts`）：对 `agent.sendMessage` / `agent.respond` 使用 `LONG_REQUEST_TIMEOUT_MS = 1200000`（与后端兜底对齐），其余方法保持 30 秒；消除前后端超时不一致导致的 `isStreaming` 提前翻转。

### 验证

- `cargo check --bin dh-desktop` / `--lib` / `-p opencode-plugin`：0 warnings。
- `npx tsc --noEmit -p tsconfig.check.json`：0 errors。
- `pnpm test`：131 个测试全部通过。
- `pnpm tauri build` 构建桌面端并启动验证。
