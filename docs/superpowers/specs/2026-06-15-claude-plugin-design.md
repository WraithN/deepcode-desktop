# Claude Code 插件设计规格

## 背景

DeepHarness Desktop 当前仅实现了 `opencode-plugin` 作为 AI 编码智能体后端。为了支持 Claude Code，需要新增一个插件，并在实现过程中抽象出通用的进程驱动能力，避免与 OpenCode 的 HTTP serve 模式重复造轮子。

## 目标

1. 在 `crates/agent-core` 中新增 `process` 模块，提供通用的进程驱动抽象：
   - `Transport` / `TransportHandle` trait
   - `StdioTransport`：基于 stdin/stdout 的 NDJSON 双向通信
   - `HttpTransport`：基于 HTTP + SSE 的通信
   - `ProcessEvent`：统一的事件模型
   - `EventMapper`：将 `ProcessEvent` 映射到 `EventSink`
2. 重构 `crates/opencode-plugin`，使其基于 `agent-core::process::HttpTransport`。
3. 新增 `crates/claude-plugin`，基于 `agent-core::process::StdioTransport` 实现 Claude Code CLI 的长期服务进程集成。
4. 前端无需改动，复用现有 WebSocket 事件契约。

## 非目标

- 不改动前端 UI、路由或状态管理。
- 不新增云端 API 调用，所有数据仍本地处理。
- 第一版不实现 Claude Code 的交互式权限回复（通过 `--permission-mode bypassPermissions` 自动绕过）。

## 架构

```
crates/
├── agent-core/
│   └── src/
│       ├── lib.rs
│       ├── plugin.rs
│       ├── instance.rs
│       ├── event_sink.rs
│       ├── logger.rs
│       ├── mcp.rs
│       └── process/              # 新增
│           ├── mod.rs
│           ├── transport.rs      # Transport / TransportHandle trait
│           ├── stdio.rs          # StdioTransport
│           ├── http.rs           # HttpTransport
│           ├── event.rs          # ProcessEvent
│           └── mapper.rs         # ProcessEvent -> EventSink
│
├── opencode-plugin/              # 重构
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── plugin.rs
│       └── instance.rs           # 使用 agent-core::process::HttpTransport
│
└── claude-plugin/                # 新增
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── plugin.rs
        ├── instance.rs           # 使用 agent-core::process::StdioTransport
        └── parser.rs             # Claude stream-json 解析
```

## Transport 抽象

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    /// 启动传输，返回一个可用于发送/接收的句柄。
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError>;

    /// 可选的端点地址（用于前端展示）。
    fn endpoint(&self) -> Option<String>;
}

#[async_trait]
pub trait TransportHandle: Send + Sync {
    /// 向 Agent 进程发送一条消息。
    async fn send(&mut self, payload: Value) -> Result<(), TransportError>;

    /// 从 Agent 进程接收下一条消息。
    async fn receive(&mut self) -> Result<Value, TransportError>;

    /// 关闭传输。
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

### StdioTransport

负责启动并维护一个长期运行的子进程，通过 stdin 写入 NDJSON，通过 stdout 读取 NDJSON。

启动命令示例：

```bash
claude -p \
  --input-format=stream-json \
  --output-format=stream-json \
  --verbose \
  --permission-mode bypassPermissions \
  --model <model> \
  --worktree <workspace>
```

实现要点：
- 使用 `tokio::process::Command` 启动子进程。
- 获取子进程的 `stdin`、`stdout`、`stderr`。
- `stderr` 重定向到 `SessionLogger`，用于调试。
- `send` 向 `stdin` 写入 JSON + `\n`。
- `receive` 从 `stdout` 按行读取并解析 JSON。
- 进程退出时尝试自动重启并恢复会话。

### HttpTransport

负责启动 `opencode serve`，并通过 HTTP + SSE 通信。

实现要点：
- 保留现有 `opencode serve --port <port> --pure` 启动逻辑。
- 将 HTTP 请求和 SSE 解析封装到 `TransportHandle`。
- `send` 对应 `POST /session/{id}/message`。
- `receive` 对应 SSE 事件流。

## 通用事件模型

```rust
pub enum ProcessEvent {
    Init { session_id: String },
    UserMessage { content: String },
    AssistantMessage { content: String },
    TextDelta { text: String },
    Thinking { content: String },
    ToolUse { name: String, input: Value },
    ToolResult { name: String, result: String, failed: bool },
    Permission { tool_name: String, action: String },
    Question { questions: Vec<QuestionItem> },
    TodoWrite { todos: Vec<TodoItem> },
    Done,
    Error { message: String },
}
```

## 事件映射

`agent-core::process::mapper` 负责将 `ProcessEvent` 转换为前端可消费的 `EventSink` 事件：

| ProcessEvent | EventSink 方法 | payload |
|---|---|---|
| `TextDelta { text }` | `agent.token` | `{ text, instance_id }` |
| `Thinking { content }` | `agent.thinking` | `{ content, id, type: "step-start", instance_id }` |
| `Permission { tool_name, action }` | `agent.permission` | `{ sessionID, interaction, conversation_id }` |
| `Question { questions }` | `agent.question` | `{ sessionID, interaction, conversation_id }` |
| `TodoWrite { todos }` | `agent.todowrite` | `{ sessionID, interaction, conversation_id }` |
| `Done` | `agent.done` | `{ instance_id }` |
| `Error { message }` | `agent.error` | `{ message, instance_id }` |

## Claude Code 输入格式

通过 stdin 向 Claude Code 发送 NDJSON 消息：

```json
{"type":"message","role":"user","content":[{"type":"text","text":"你好"}]}
```

多轮对话时，每次发送一条 `message` 即可。Claude Code 进程会维护自身内存中的上下文。

## 会话管理

- `ClaudeInstance` 内部维护 `conversation_id -> claude_session_id` 映射。
- 创建 `ClaudeInstance` 时立即启动 Claude Code 进程（eager start）。
- 第一次 `send_message` 时，从 stdout 读取 `init` 事件获取 `session_id`，保存映射。
- 后续 `send_message` 直接复用同一进程，发送 `message` 事件。
- 进程意外退出时：
  - 如果已有 `session_id`，使用 `--resume <session_id>` 重启并恢复。
  - 如果没有，视为新会话重新启动。

## InstanceConfig 扩展

```rust
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub session_id: Option<String>,

    // 新增字段
    pub model: Option<String>,
    pub permission_mode: Option<String>,
}
```

- `model`：映射到 `--model`，如 `sonnet`、`opus`、`haiku`。
- `permission_mode`：映射到 `--permission-mode`，默认 `bypassPermissions`。

## 错误处理

| 场景 | 处理方式 |
|---|---|
| 进程启动失败 | `InstanceError::ProcessError` |
| stdout 行解析失败 | 记录 `SessionLogger`，跳过该事件 |
| 进程崩溃 | 自动重启，尝试 `--resume` 恢复 |
| receive 超时 | 返回 `InstanceError::ReceiveTimeout` |
| 未知 event type | 记录并忽略 |

## OpenCode 重构要点

1. 删除现有的 HTTP 客户端和 SSE 解析代码，改为使用 `agent-core::process::HttpTransport`。
2. 将 OpenCode 的 SSE 事件解析为 `ProcessEvent`。
3. 通过通用 `EventMapper` 转发到 `EventSink`。
4. 保持前端事件格式不变，避免回归。

## Claude 插件实现要点

1. `ClaudePlugin::is_installed` 检查 `claude --version` 是否成功。
2. `ClaudePlugin::create_instance` 构造 `ClaudeInstance`。
3. `ClaudeInstance` 在创建时启动 `StdioTransport`。
4. `send_message` 发送 `message` 事件并消费 `ProcessEvent`。
5. `respond` 同样发送 `message` 事件。
6. `stop` 关闭 `TransportHandle` 并终止子进程。

## 测试策略

1. **单元测试**
   - `agent-core::process`：mock transport 测试 `send` / `receive` / 重启逻辑。
   - `claude-plugin::parser`：stream-json 行解析测试。
   - `opencode-plugin`：重构后事件映射测试。

2. **集成测试**
   - 启动 `claude-plugin` 并发送简单 prompt，验证能收到 `agent.token` / `agent.done`。
   - 启动 `opencode-plugin` 并发送 prompt，验证行为与重构前一致。

3. **端到端验证**
   - `pnpm tauri build` 成功。
   - `bash run-desktop.sh` 启动正常。
   - 在桌面应用中创建 Claude 智能体并发送消息。

## 风险与缓解

| 风险 | 缓解措施 |
|---|---|
| Claude Code `--input-format=stream-json` 格式未完全文档化 | 先实现最小可运行版本，根据实际输出调整 parser |
| 一次性重构 OpenCode 引入回归 | 保持事件格式不变，集成测试覆盖主要路径 |
| 长期进程稳定性 | 实现进程崩溃自动重启和 resume |
| 单文件超过 600 行 | 将 parser / transport / mapper 拆分到独立文件 |

## 验收标准

- [ ] `cargo check` 全 workspace 0 warnings。
- [ ] `pnpm lint` 通过。
- [ ] `pnpm tauri build` 成功。
- [ ] 桌面应用可创建 Claude 智能体并发送消息。
- [ ] OpenCode 智能体行为与重构前一致。
