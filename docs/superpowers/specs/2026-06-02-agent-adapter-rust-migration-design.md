# Agent 适配层 Rust 迁移设计文档

> 日期：2026-06-02
> 目标：将 TypeScript 中的 OpenCode 智能体适配层迁移至 Rust，定义统一 AgentPlugin trait，每个智能体独立 crate，Rust 管理全部实例状态。

---

## 1. 背景与目标

### 当前现状
- TypeScript 侧 `src/agents/opencode/` 直接通过 Tauri shell 插件调用 `opencode` CLI
- Rust 侧 `sidecar_manager.rs` 和 `agent_db.rs` 已存在但未被前端使用
- 前端 `AgentManager` 以 Singleton 管理实例状态和进程，逻辑分散

### 迁移目标
1. **Rust 侧统管实例生命周期** — 创建、启动、停止、状态查询全部在 Rust 完成
2. **Plugin = 模板，Instance = 实例** — 支持同类型多实例（如 2 个 OpenCode）
3. **Tauri Command + Event 通信** — 前端 invoke 发送消息，Rust emit 推送事件流
4. **Session Logs 双写** — Rust 日志异步推前端 + 写入本地 SQLite
5. **可扩展** — 未来新增智能体只需新增一个 crate 实现 trait 即可

---

## 2. 架构设计

### 2.1 Rust Workspace 结构

```
src-tauri/
  Cargo.toml                    # workspace 根
  src/
    main.rs                     # 注册 commands + 初始化 AgentService + SessionLogger
    lib.rs                      # 模块聚合
    commands/
      agent.rs                  # Tauri command handlers（薄层，只负责参数校验和转发）
      session_log.rs            # session_log_load 等 command
    service/
      agent_service.rs          # AgentService 单例：PluginRegistry + InstanceRegistry
      plugin_registry.rs        # 插件注册表
      instance_registry.rs      # 实例注册表
    models/
      agent.rs                  # InstanceStatus, InstanceConfig, InstanceInfo 等
      event.rs                  # AgentEvent 枚举
      log.rs                    # SessionLogEntry, LogLevel
  crates/
    agent-core/                 # trait 定义 + 公共类型 + SessionLogger
      src/
        lib.rs
        plugin.rs               # AgentPlugin trait
        instance.rs             # AgentInstance trait
        event.rs                # AgentEvent
        logger.rs               # SessionLogger（异步双写）
        error.rs                # PluginError, InstanceError
    agent-runtime/              # 通用进程管理（提取自 sidecar_manager.rs）
      src/
        lib.rs
        process.rs              # ProcessHandle, spawn, kill
        health_check.rs         # 健康检查、僵尸进程清理
    opencode-plugin/            # OpenCode 具体实现
      src/
        lib.rs
        plugin.rs               # OpencodePlugin 实现 AgentPlugin
        instance.rs             # OpencodeInstance 实现 AgentInstance
        parser.rs               # JSON-line 解析（从 TS parser.ts 迁移）
        mapper.rs               # 原始事件 → AgentEvent 映射
```

### 2.2 Core Traits

```rust
// agent-core/src/plugin.rs
pub trait AgentPlugin: Send + Sync {
    fn key(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_installed(&self) -> bool;
    fn create_instance(
        &self,
        config: InstanceConfig,
    ) -> Result<Box<dyn AgentInstance>, PluginError>;
}

// agent-core/src/instance.rs
#[async_trait]
pub trait AgentInstance: Send + Sync {
    fn id(&self) -> &str;
    fn status(&self) -> InstanceStatus;
    async fn send_message(&self, message: &str) -> Result<(), InstanceError>;
    async fn stop(&self) -> Result<(), InstanceError>;
}

// agent-core/src/event.rs
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse { tool_name: String, args: Value },
    ToolResult { tool_name: String, result: String, failed: bool },
    AskPermission { message: String, tool_name: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
}
```

### 2.3 AgentService（Rust 单例）

```rust
pub struct AgentService {
    plugins: PluginRegistry,           // HashMap<&'static str, Box<dyn AgentPlugin>>
    instances: InstanceRegistry,       // HashMap<String, Arc<Mutex<dyn AgentInstance>>>
    logger: Arc<SessionLogger>,
}
```

**职责：**
- 初始化时注册所有 Plugin（静态编译时注册）
- `create_instance`：根据 plugin_key 创建实例，分配 ID，启动进程
- `send_message`：查找实例，委托给 `AgentInstance::send_message`
- `stop_instance`：查找实例，调用 `stop`
- `get_instance` / `list_instances`：查询状态

---

## 3. 通信协议

### 3.1 Tauri Commands（前端 → Rust）

| Command | 输入 | 输出 | 说明 |
|---------|------|------|------|
| `agent_list_plugins` | — | `Vec<PluginInfo>` | 可用智能体类型列表 |
| `agent_create_instance` | `{ plugin_key, name, workspace }` | `InstanceInfo` | 创建实例 |
| `agent_send_message` | `{ instance_id, message, conversation_id }` | `()` | 发送消息 |
| `agent_stop_instance` | `{ instance_id }` | `()` | 停止实例 |
| `agent_get_instance` | `{ instance_id }` | `InstanceInfo` | 查询实例 |
| `agent_list_instances` | — | `Vec<InstanceInfo>` | 列出所有实例 |
| `session_log_load` | `{ conversation_id }` | `Vec<SessionLogEntry>` | 加载历史日志 |

### 3.2 Tauri Events（Rust → 前端）

| Event | Payload | 说明 |
|-------|---------|------|
| `agent:event` | `{ instance_id: string, event: AgentEvent }` | 流式事件推送 |
| `agent:status_changed` | `{ instance_id: string, status: InstanceStatus }` | 实例状态变更 |
| `session:log` | `SessionLogEntry` | 日志推送 |

---

## 4. 数据流

### 4.1 一次完整对话

```
前端 WorkspacePage
  │
  ▼
invoke("agent_send_message", { instance_id, message, conversation_id })
  │
  ▼
Command Handler (commands/agent.rs)
  │
  ▼
AgentService::send_message(instance_id, message, conversation_id)
  │  ├── logger.log(..., "send_message called", ...)
  │
  ▼
InstanceRegistry::get(instance_id) → Arc<Mutex<dyn AgentInstance>>
  │
  ▼
OpencodeInstance::send_message(message)
  │
  ├── 1. 构建 CLI: opencode run --format json --dir <workspace> <message>
  │     logger.log(..., "CLI args built", ...)
  │
  ├── 2. agent_runtime::spawn_command(cmd, workspace)
  │      │
  │      ▼
  │   tokio::process::Command 异步执行
  │      │
  │      ▼
  │   stdout 逐行读取
  │      │
  │      ▼
  │   parse_opencode_json_line(line)
  │      │
  │      ▼
  │   map_to_agent_event(raw) → AgentEvent
  │      │
  │      ▼
  │   emit("agent:event", { instance_id, event })
  │      │
  │      ▼
  │   前端 listen("agent:event") → 更新 ChatPanel UI
  │
  └── 3. 进程结束
        emit("agent:status_changed", { instance_id, status: Stopped })
        logger.log(..., "process exited", ...)
```

### 4.2 关键设计决策

1. **事件流单向推送** — `send_message` 返回 `Result<(), Error>` 只表示"已提交"，真实响应用 Event 推送
2. **解析在 Plugin 内部** — 每个 Plugin 把原生 CLI 输出转成统一 `AgentEvent`，`agent-core` 不感知具体协议
3. **进程由 `agent-runtime` 管理** — Plugin 只关心"执行什么命令"，`agent-runtime` 管 spawn/kill/health_check

---

## 5. Session Logs 双写设计

### 5.1 Rust 侧 SessionLogger

```rust
pub struct SessionLogger {
    sender: mpsc::UnboundedSender<LogEntry>,
}

impl SessionLogger {
    pub fn new(app_handle: AppHandle, db_conn: rusqlite::Connection) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        tokio::spawn(async move {
            while let Some(entry) = rx.recv().await {
                // 1. emit 到前端（异步，不阻塞）
                let _ = app_handle.emit("session:log", &entry);
                
                // 2. 写入本地 SQLite（异步，不阻塞）
                let _ = db_conn.execute(
                    "INSERT INTO session_logs (...) VALUES (...)",
                    params![...],
                );
            }
        });
        
        Self { sender: tx }
    }
    
    pub fn log(&self, conversation_id: &str, level: LogLevel, source: &str, message: &str, payload: Option<Value>) {
        let _ = self.sender.send(LogEntry { ... });
    }
}
```

### 5.2 本地存储表结构

```sql
CREATE TABLE IF NOT EXISTS session_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    source TEXT NOT NULL,
    message TEXT NOT NULL,
    payload TEXT
);
CREATE INDEX idx_session_logs_conversation ON session_logs(conversation_id, timestamp);
```

### 5.3 日志来源

| 来源 | 示例 | 级别 |
|------|------|------|
| `opencode-plugin` | "构建 CLI 参数: opencode run ..." | `info` |
| `opencode-plugin` | "解析事件: Thinking{...}" | `debug` |
| `agent-runtime` | "Spawn 进程 pid=12345" | `info` |
| `agent-runtime` | "进程异常退出: exit code 1" | `error` |
| `agent-service` | "创建实例: opencode-1" | `info` |

### 5.4 前端加载历史日志

`WorkspacePage` 切换会话时调用 `session_log_load(conversation_id)`，合并内存新日志 + 数据库历史日志。

---

## 6. 前后端类型对齐

### Rust `AgentEvent`（agent-core/src/event.rs）

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse { tool_name: String, args: Value },
    ToolResult { tool_name: String, result: String, failed: bool },
    AskPermission { message: String, tool_name: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
}
```

### 前端 `AgentEvent`（保留 src/agents/types.ts）

```typescript
export type AgentEvent =
  | { type: 'thinking'; content: string }
  | { type: 'text_delta'; content: string }
  | { type: 'tool_use'; toolName: string; args: unknown }
  | { type: 'tool_result'; toolName: string; result: string; failed: boolean }
  | { type: 'ask_permission'; message: string; toolName: string }
  | { type: 'ask_user'; questions: string[] }
  | { type: 'error'; message: string }
  | { type: 'done' };
```

**字段命名映射：** Rust `tool_name` / `toolName` 等通过 serde `rename_all = "camelCase"` 自动转换。

---

## 7. 迁移步骤

### Phase 1：搭建 Rust Workspace 骨架
1. `src-tauri/Cargo.toml` 改为 workspace，定义 members
2. 新建 `crates/agent-core/`：trait + 公共类型 + SessionLogger 骨架
3. 新建 `crates/agent-runtime/`：进程管理，从 `sidecar_manager.rs` 提取通用逻辑
4. 新建 `crates/opencode-plugin/`：空壳，依赖 agent-core + agent-runtime
5. 主 crate `src-tauri` 依赖上述 crates，编译通过

### Phase 2：迁移 OpenCode 逻辑
1. `opencode-plugin/src/parser.rs`：翻译 `src/agents/opencode/parser.ts`
2. `opencode-plugin/src/mapper.rs`：翻译 `adapter.ts` 中的 `mapToAgentEvent`
3. `opencode-plugin/src/instance.rs`：`OpencodeInstance` 实现 `AgentInstance`
4. `opencode-plugin/src/plugin.rs`：`OpencodePlugin` 实现 `AgentPlugin`
5. `agent-runtime` 实现 `spawn_command`、`kill`、`health_check`

### Phase 3：暴露 Tauri Command + Event
1. `src-tauri/src/commands/agent.rs`：实现 6 个 agent command
2. `src-tauri/src/commands/session_log.rs`：实现 `session_log_load`
3. `main.rs`：初始化 `AgentService` + `SessionLogger`，注册到 Tauri State
4. `main.rs`：注册所有 commands 到 `invoke_handler`

### Phase 4：前端适配
1. 新建 `src/hooks/use-agent-service.ts`：封装 `invoke` + `listen`
2. 新建 `src/hooks/use-session-log-rust.ts`：监听 `session:log` event
3. `WorkspacePage.tsx`：替换 `agentManager.sendMessage` → `agentSendMessage()`
4. `WorkspacePage.tsx`：切换会话时调用 `session_log_load()`
5. 删除 `src/agents/opencode/` 目录（adapter、parser、test）
6. 精简 `src/agents/manager.ts` 和 `src/agents/registry.ts` 为薄封装

### Phase 5：验证
1. `pnpm tauri build` 编译通过
2. 启动应用，验证 OpenCode 对话正常（发送消息 → 接收事件流 → 渲染）
3. 验证 Session Logs 中同时包含前端和 Rust 日志
4. 验证应用重启后 `session_log_load` 能加载历史日志

---

## 8. 废弃项

迁移完成后以下文件/模块废弃：
- `src/agents/opencode/adapter.ts` → 逻辑移至 `opencode-plugin/src/instance.rs`
- `src/agents/opencode/parser.ts` → 逻辑移至 `opencode-plugin/src/parser.rs`
- `src/agents/opencode/*.test.ts` → 测试移至 Rust crate 内
- `src/agents/registry.ts` → 功能由 Rust `PluginRegistry` 取代
- `src-tauri/src/sidecar_manager.rs` → 通用逻辑提取到 `agent-runtime`，OpenCode 特化逻辑废弃
- `src-tauri/src/agent_db.rs` → 合并到主 `app.db`，表结构由 `AgentService` 管理

---

## 9. 风险与注意事项

1. **async_trait vs 原生 async trait** — Rust 1.75+ 支持原生 async trait，但 Tauri 的 `State` 要求 `Send + Sync`，建议先用 `async-trait` crate 保证兼容性
2. **tokio vs std process** — `agent-runtime` 必须使用 `tokio::process` 才能与 Tauri 的 tokio runtime 兼容
3. **前端事件监听生命周期** — `useAgentService` hook 必须在组件 unmount 时正确 `unlisten`，避免内存泄漏
4. **日志写入性能** — SQLite 写入在高频日志场景下可能成为瓶颈，后续可考虑批量写入或 ring buffer
