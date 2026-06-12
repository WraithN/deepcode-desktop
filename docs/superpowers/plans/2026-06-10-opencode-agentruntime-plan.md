# OpenCode Agent Runtime 独立化 — 实施计划

> **日期**: 2026-06-10
> **范围**: Phase 1 + Phase 2（Tauri 解耦 + 插件系统激活）
> **依赖**: 设计文档 `../specs/2026-06-10-opencode-agentruntime-design.md`

---

## 实施概览

| 阶段 | 任务 | 预计文件数 | 风险 |
|------|------|-----------|------|
| **P1-1** | 清理：`OpencodeService` 删除 + `agent-runtime` 删除 | 2 | 低 |
| **P1-2** | `EventSink` trait 抽象 + `SessionLogger` 解耦 | 3 | 中（接口改动） |
| **P1-3** | Crate 迁移：`agent-core` → `crates/` | 5 | 中（workspace 结构） |
| **P1-4** | Crate 迁移：`opencode-plugin` → `crates/` | 4 | 中 |
| **P1-5** | `agent-core` MCP 去重：使用 `dh-core` MCP 栈 | 2 | 低 |
| **P1-6** | Tauri 端适配：`WebSocketEventSink` + `AgentService` 包装 | 3 | 中 |
| **P2-1** | `OpencodePlugin` 重构：MCP → HTTP/SSE | 4 | 高（核心逻辑重写） |
| **P2-2** | `AgentService` 多实例管理 + 状态机 | 3 | 中 |
| **P2-3** | WebSocket handler 路由切换 | 2 | 中 |
| **P2-4** | CLI `dh gatewayd start <agent-type>` | 2 | 低 |
| **P2-5** | 验证与测试 | 3 | 低 |

---

## 详细实施步骤

### P1-1: 清理遗留代码

**目标**: 删除不再需要的 `OpencodeService` 和 `agent-runtime`

1. **删除 `src-tauri/src/service/opencode_service.rs`**
   - 移除所有 `opencode_service` 模块引用
   - 在 `src-tauri/src/service/mod.rs` 中删除 `pub mod opencode_service`
   - 在 `src-tauri/src/main.rs` 中移除 `OpencodeService` 初始化

2. **删除 `src-tauri/crates/agent-runtime/`**
   - 完全删除目录
   - 从 `src-tauri/Cargo.toml` 工作区列表中移除 `agent-runtime`

3. **验证**: `cargo check --workspace` 确保无 broken references

---

### P1-2: EventSink 抽象

**目标**: 将 `agent-core` 从 Tauri 解耦

1. **`crates/agent-core/src/event_sink.rs`**（新建）
   ```rust
   use serde_json::Value;
   use std::sync::Arc;

   pub trait EventSink: Send + Sync {
       fn emit(&self, event_type: &str, payload: Value);
   }

   pub type DynEventSink = Arc<dyn EventSink>;
   ```

2. **`crates/agent-core/src/session_logger.rs`**（重构现有）
   - 将 `SessionLogger` 中的 `tauri::AppHandle` 替换为 `DynEventSink`
   - 删除 `tauri::Emitter` 导入
   - 保留 SQLite + 文件写入逻辑
   - 新增 `WebSocketEventSink`（在 Tauri 侧实现，不放在 agent-core 中）

3. **`crates/agent-core/src/lib.rs`**
   - 导出 `EventSink` trait
   - 移除所有 Tauri 依赖

4. **验证**: `cargo check -p agent-core` 确认 0 Tauri 引用

---

### P1-3: agent-core 迁移

**目标**: 将 `agent-core` 从 `src-tauri/crates/` 移到 `crates/`

1. **文件复制**:
   ```bash
   mkdir -p crates/agent-core/src
   cp -r src-tauri/crates/agent-core/src/* crates/agent-core/src/
   ```

2. **`crates/agent-core/Cargo.toml`**（新建）
   ```toml
   [package]
   name = "agent-core"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   dh-core = { path = "../dh-core" }
   tokio = { version = "1", features = ["full"] }
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   async-trait = "0.1"
   thiserror = "1.0"
   uuid = { version = "1", features = ["v4"] }
   ```
   **注意**: 无 `tauri` 依赖！

3. **代码调整**:
   - 所有 `use tauri::*` 替换为 `EventSink`
   - `SessionLogger::new()` 签名改为 `new(sink: DynEventSink, db_path: PathBuf)`
   - MCP 客户端模块删除（后续使用 dh-core 的）

4. **顶层 `Cargo.toml` 工作区更新**:
   ```toml
   members = [
       "crates/dh-core",
       "crates/dh-platform",
       "crates/dh-db",
       "crates/agent-core",
       # ...
   ]
   ```

5. **`src-tauri/Cargo.toml` 更新**:
   - 添加 `agent-core = { path = "../crates/agent-core" }` 依赖
   - 移除 `agent-core` 本地 workspace 引用

---

### P1-4: opencode-plugin 迁移

**目标**: 将 `opencode-plugin` 移到 `crates/`

1. **文件复制**:
   ```bash
   mkdir -p crates/opencode-plugin/src
   cp -r src-tauri/crates/opencode-plugin/src/* crates/opencode-plugin/src/
   ```

2. **`crates/opencode-plugin/Cargo.toml`**（新建）
   ```toml
   [package]
   name = "opencode-plugin"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   agent-core = { path = "../agent-core" }
   dh-core = { path = "../dh-core" }
   tokio = { version = "1", features = ["full", "process"] }
   reqwest = { version = "0.12", features = ["json", "stream"] }
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   async-trait = "0.1"
   thiserror = "1.0"
   tracing = "0.1"
   ```
   **注意**: 无 `tauri` 依赖，有 `reqwest`！

3. **代码清理**:
   - 删除 `mcp_adapter.rs`（MCP 相关全部移除）
   - 删除 `AppHandle` 相关引用
   - `OpencodePlugin` 构造函数改为 `new()`（不再传 `AppHandle`）

4. **顶层工作区更新**

---

### P1-5: MCP 去重

**目标**: 删除 `agent-core` 中的 MCP 客户端，复用 `dh-core`

1. **删除 `crates/agent-core/src/mcp/`** 目录

2. **`crates/agent-core/src/lib.rs`**:
   - 移除 `pub mod mcp`
   - 从 dh-core 导出需要的 MCP 类型（如有）

3. **`dh-core` 补充**（如需）:
   - 检查 `dh-core/src/mcp/` 是否包含 `agent-core` 所需的全部接口
   - 如有缺失，补充到 `dh-core`

---

### P1-6: Tauri 端适配

**目标**: 在 Desktop 侧实现 WebSocket 事件推送

1. **`src-tauri/src/event_sink.rs`**（新建）
   ```rust
   use agent_core::EventSink;
   use serde_json::Value;

   pub struct WebSocketEventSink {
       session_manager: Arc<crate::gateway::SessionManager>,
   }

   impl EventSink for WebSocketEventSink {
       fn emit(&self, event_type: &str, payload: Value) {
           let msg = serde_json::json!({
               "jsonrpc": "2.0",
               "method": event_type,
               "params": payload
           });
           self.session_manager.broadcast(msg.to_string());
       }
   }
   ```

2. **`src-tauri/src/service/agent_service.rs`**（重构）
   - 移除 `tauri::AppHandle` 引用
   - 改为持有 `Arc<dyn EventSink>`
   - 初始化时传入 `WebSocketEventSink`

3. **`src-tauri/src/main.rs`**:
   - `AgentService` 初始化改为 `AgentService::new(Arc::new(WebSocketEventSink::new(session_mgr)))`

---

### P2-1: OpencodePlugin HTTP/SSE 重构

**目标**: 将 `opencode-plugin` 从 MCP 改为 HTTP/SSE

1. **`crates/opencode-plugin/src/opencode_plugin.rs`**（重写）
   ```rust
   pub struct OpencodePlugin {
       client: reqwest::Client,
   }

   impl AgentPlugin for OpencodePlugin {
       fn key(&self) -> &str { "opencode" }
       fn name(&self) -> &str { "OpenCode" }
       
       fn is_installed(&self) -> bool {
           Command::new("opencode")
               .arg("--version")
               .output()
               .map(|o| o.status.success())
               .unwrap_or(false)
       }
       
       fn create_instance(
           &self,
           config: InstanceConfig,
           event_sink: DynEventSink,
       ) -> Box<dyn AgentInstance> {
           Box::new(OpencodeInstance::new(config, event_sink, self.client.clone()))
       }
   }
   ```

2. **`crates/opencode-plugin/src/opencode_instance.rs`**（新建）
   - 启动 `opencode serve --port <port>`
   - SSE 事件循环（`GET /v1/events`）
   - 发送消息（`POST /v1/messages`）
   - 事件解析（复用 `parser.rs` 中的逻辑）

3. **`crates/opencode-plugin/src/lib.rs`**:
   - 导出 `OpencodePlugin`
   - 删除 MCP 相关模块

---

### P2-2: AgentService 多实例管理

**目标**: 支持并行运行多个 agent 实例

1. **`crates/agent-core/src/agent_service.rs`**（新建或重构）
   ```rust
   pub struct AgentService {
       plugins: HashMap<String, Box<dyn AgentPlugin>>,
       instances: RwLock<HashMap<String, AgentInstanceHandle>>,
       event_sink: DynEventSink,
   }

   impl AgentService {
       pub fn register_plugin(&mut self, plugin: Box<dyn AgentPlugin>);
       pub async fn create_instance(&self, plugin_type: &str, config: InstanceConfig) -> Result<String, Error>;
       pub async fn send_message(&self, instance_id: &str, message: &str) -> Result<(), Error>;
       pub async fn stop_instance(&self, instance_id: &str) -> Result<(), Error>;
       pub fn list_instances(&self) -> Vec<InstanceInfo>;
   }
   ```

2. **实例状态机**:
   ```rust
   pub enum InstanceStatus {
       Starting,
       Running,
       Stopping,
       Stopped,
       Error(String),
   }
   ```

---

### P2-3: WebSocket Handler 路由切换

**目标**: 让 WebSocket 请求走 `AgentService`

1. **`src-tauri/src/gateway/handlers/agent.rs`**（修改）
   - `agent.createInstance` → 调用 `AgentService::create_instance`
   - `agent.sendMessage` → 调用 `AgentService::send_message`
   - `agent.stopInstance` → 调用 `AgentService::stop_instance`
   - 事件推送使用 `EventSink`

2. **删除 `src-tauri/src/service/opencode_service.rs` 的引用**（已在 P1-1 完成）

---

### P2-4: CLI 增强

**目标**: 支持 `dh gatewayd start <agent-type>`

1. **`apps/cli/src/commands/gatewayd.rs`**:
   ```rust
   pub enum GatewaydCommands {
       Start {
           #[arg(long)]
           daemon: bool,
           /// Agent types to auto-start
           agent_types: Vec<String>,
       },
   }
   ```

2. **`apps/gatewayd/src/main.rs`**:
   - 接收 `agent_types` 参数
   - 初始化 `AgentService`
   - 自动创建配置的 agent 实例

3. **验证**: `dh gatewayd start opencode` 能启动 gatewayd 并初始化 OpenCode 实例

---

### P2-5: 验证与测试

**目标**: 确保所有功能正常

1. **编译验证**:
   ```bash
   cargo check --workspace
   npx tsc --noEmit -p tsconfig.check.json
   ```

2. **单元测试**:
   ```bash
   cargo test -p agent-core
   cargo test -p opencode-plugin
   ```

3. **集成测试**:
   - 启动 Desktop (`pnpm tauri dev`)
   - 登录 → 选择 OpenCode → 发送消息
   - 验证 WebSocket 事件正常
   - 验证 `session.log` 正常

4. **CLI 测试**:
   ```bash
   dh gatewayd start opencode
   curl http://localhost:2345/health
   # 验证 agent 实例已创建
   ```

---

## 时间表

| 步骤 | 任务 | 预计耗时 |
|------|------|---------|
| P1-1 | 清理 | 30 min |
| P1-2 | EventSink 抽象 | 45 min |
| P1-3 | agent-core 迁移 | 60 min |
| P1-4 | opencode-plugin 迁移 | 45 min |
| P1-5 | MCP 去重 | 30 min |
| P1-6 | Tauri 适配 | 45 min |
| **P1 总计** | | **~4.5h** |
| P2-1 | HTTP/SSE 重构 | 90 min |
| P2-2 | 多实例管理 | 60 min |
| P2-3 | Handler 切换 | 45 min |
| P2-4 | CLI 增强 | 45 min |
| P2-5 | 验证测试 | 60 min |
| **P2 总计** | | **~5h** |
| **总计** | | **~9.5h** |

---

## 依赖关系图

```
P1-1 (清理)
  │
  ▼
P1-2 (EventSink) ──► P1-3 (agent-core迁移) ──► P1-4 (opencode-plugin迁移)
  │                                              │
  ▼                                              ▼
P1-6 (Tauri适配) ◄────────────────────────────── P1-5 (MCP去重)
  │
  ▼
P2-1 (HTTP/SSE重构) ──► P2-2 (多实例) ──► P2-3 (Handler切换)
                                             │
                                             ▼
                                          P2-4 (CLI)
                                             │
                                             ▼
                                          P2-5 (验证)
```

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Rust 1.95 ICE（目录模块） | 编译失败 | 所有模块使用扁平结构（`mod foo { ... }` inline），不使用 `mod.rs` |
| WebSocket 事件格式不兼容 | 前端无法接收 | 保持 JSON-RPC 格式与现有一致，集成测试验证 |
| `opencode serve` API 变更 | HTTP/SSE 协议不匹配 | 使用 `opencode --version` 检查，预留版本适配逻辑 |
| 多实例资源冲突 | 端口/进程冲突 | 自动端口分配，实例 ID 隔离 |
| Workspace 依赖循环 | cargo 编译失败 | 严格分层：`opencode-plugin` → `agent-core` → `dh-core` |
