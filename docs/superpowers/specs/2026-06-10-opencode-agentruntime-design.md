# OpenCode Agent Runtime 独立化设计文档

> **日期**: 2026-06-10
> **主题**: 将 OpenCode 封装从 Desktop 独立为 agent runtime，支持 gatewayd 共享
> **状态**: 已确认

---

## 1. 需求概述

### 1.1 背景

当前 `opencode-plugin` 位于 `src-tauri/crates/` 子工作区中，通过 MCP 协议（`opencode mcp-server`）与 OpenCode 通信，且深度耦合 Tauri（使用 `AppHandle`/`Emitter` 推送事件）。这导致：

1. `opencode-plugin` 无法被 `gatewayd` 复用
2. 遗留的 `OpencodeService`（HTTP/SSE）才是实际生产路径，插件系统是"骨架"
3. `agent-runtime` crate 是孤儿代码（被依赖但未被使用）
4. MCP 客户端有两套重复实现（`agent-core/src/mcp/` 和 `crates/dh-core/src/mcp/`）

### 1.2 目标

**Phase 1：解耦与迁移**
1. 将 `agent-core` 和 `opencode-plugin` 从 `src-tauri/crates/` 迁移到顶层 `crates/`
2. 删除 `agent-runtime` 孤儿 crate
3. 删除 `OpencodeService` 遗留代码
4. 将 `agent-core` 与 Tauri 解耦：用 `WebSocketEventSink` 替代 `AppHandle::emit`

**Phase 2：插件系统激活**
5. 重构 `OpencodePlugin`：从 MCP（`opencode mcp-server`）改为 HTTP/SSE（`opencode serve`）
6. 让 WebSocket handler 走 `AgentService` → `OpencodePlugin` 路径（替代 `OpencodeService`）
7. `AgentService` 支持多实例并行管理

**Phase 3：Gatewayd 集成（本期不实现，架构预留）**
8. `gatewayd` 初始化 `AgentService`，暴露 HTTP/WebSocket agent API
9. CLI 新增 `dh gatewayd start opencode/claudecode/codex` 命令

### 1.3 非目标

- 本期不实现 Claude Code / Codex 插件（仅预留架构，硬编码 3 个类型但只实现 OpenCode）
- 本期不实现 gatewayd 的 agent HTTP API（Phase 3）
- 不替换 gatewayd 的 LLM API 代理功能
- 不修改前端 React 代码（WebSocket 事件格式保持兼容）

---

## 2. 架构设计

### 2.1 目标架构

```
┌─────────────────────────────────────────────────────────────┐
│                     DeepHarness Desktop                      │
│                                                              │
│  ┌─────────────┐      WebSocket      ┌──────────────────┐  │
│  │  React SPA  │ ◄─────────────────► │  Tauri Gateway   │  │
│  │  (前端)      │     JSON-RPC        │  (WebSocket srv) │  │
│  └─────────────┘                     └────────┬─────────┘  │
│                                               │            │
│                                               ▼            │
│                                    ┌─────────────────────┐ │
│                                    │   AgentService      │ │
│                                    │   (插件注册表 +       │ │
│                                    │    实例生命周期管理)   │ │
│                                    └────────┬────────────┘ │
│                                             │              │
│                                             ▼              │
│                                    ┌─────────────────────┐ │
│                                    │  OpencodePlugin      │ │
│                                    │  (HTTP/SSE 客户端)   │ │
│                                    └────────┬────────────┘ │
│                                             │              │
│                                             ▼              │
│                                    ┌─────────────────────┐ │
│                                    │  opencode serve      │ │
│                                    │  (本地 HTTP 服务器)   │ │
│                                    └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 模块边界

| 模块 | 职责 | 变更 |
|------|------|------|
| `crates/agent-core/` | AgentPlugin/AgentInstance traits + WebSocket EventSink + 实例管理 | 从 `src-tauri/crates/` 迁移，删除 Tauri 依赖 |
| `crates/opencode-plugin/` | OpenCode HTTP/SSE 插件实现 | 从 `src-tauri/crates/` 迁移，删除 MCP 依赖，改用 HTTP/SSE |
| `src-tauri/src/service/agent_service.rs` | Desktop 的 AgentService 包装 | 初始化 AgentService，注册插件 |
| `src-tauri/src/service/opencode_service.rs` | 遗留 HTTP 客户端 | **删除** |
| `src-tauri/src/gateway/handlers/agent.rs` | WebSocket agent handler | 路由到 AgentService（替代 OpencodeService） |
| `apps/cli/src/commands/gatewayd.rs` | CLI 命令 | 新增 `start <agent-type>` 支持 |

---

## 3. Tauri 解耦设计

### 3.1 EventSink 抽象

```rust
pub trait EventSink: Send + Sync {
    fn emit(&self, event_type: &str, payload: Value);
}
```

### 3.2 Desktop 实现：WebSocketEventSink

```rust
pub struct WebSocketEventSink {
    session_manager: Arc<gateway::SessionManager>,
}

impl EventSink for WebSocketEventSink {
    fn emit(&self, event_type: &str, payload: Value) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": event_type,
            "params": payload
        });
        self.session_manager.broadcast(msg);
    }
}
```

### 3.3 事件映射

| 原 Tauri 事件 | WebSocket 方法 | 前端消费 |
|--------------|---------------|---------|
| `session:log` | `session.log` | SessionLogDrawer |
| `agent:event` | `agent.event` | ChatPanel |
| `agent:status_changed` | `agent.status_changed` | AgentIcon |

---

## 4. Crate 迁移与结构调整

### 4.1 目录变更

```
当前结构：
src-tauri/crates/
├── agent-core/
├── agent-runtime/     ← 删除
└── opencode-plugin/

target 结构：
crates/
├── dh-core/           ← 已有
├── dh-platform/       ← 已有
├── dh-db/             ← 已有
├── agent-core/        ← 从 src-tauri/crates/ 迁移
└── opencode-plugin/   ← 从 src-tauri/crates/ 迁移
```

### 4.2 Workspace 更新

顶层 `Cargo.toml`：
```toml
members = [
    "crates/dh-core",
    "crates/dh-platform",
    "crates/dh-db",
    "crates/agent-core",
    "crates/opencode-plugin",
    "apps/gatewayd",
    "apps/cli",
]
```

### 4.3 依赖关系

```
opencode-plugin
├── agent-core (traits, EventSink)
├── dh-core (TokenUsage, types)
├── reqwest, tokio, serde, serde_json

agent-core
├── dh-core (types)
└── tokio, serde, serde_json, async-trait, thiserror
```

**agent-core 不再依赖 tauri！**

---

## 5. OpencodePlugin 重构

### 5.1 架构变更

| 方面 | 旧（MCP） | 新（HTTP/SSE） |
|------|----------|---------------|
| 启动命令 | `opencode mcp-server` | `opencode serve --port <port>` |
| 通信协议 | MCP stdio JSON-RPC | HTTP POST + SSE |
| 事件来源 | MCP notifications/message | SSE event stream |
| 依赖 | `agent-core/src/mcp/` | `reqwest` HTTP client |

### 5.2 OpencodeInstance 实现

```rust
pub struct OpencodeInstance {
    id: String,
    config: InstanceConfig,
    event_sink: Arc<dyn EventSink>,
    client: reqwest::Client,
    base_url: String,
}

impl AgentInstance for OpencodeInstance {
    async fn start(&mut self) -> Result<(), InstanceError> {
        let port = find_free_port();
        self.base_url = format!("http://127.0.0.1:{}", port);
        
        tokio::process::Command::new("opencode")
            .args(&["serve", "--port", &port.to_string()])
            .spawn()
            .map_err(|e| InstanceError::SpawnFailed(e.to_string()))?;
        
        wait_for_healthy(&self.base_url).await?;
        tokio::spawn(self.sse_event_loop());
        
        Ok(())
    }
    
    async fn send_message(&self, message: &str) -> Result<(), InstanceError> {
        self.client
            .post(format!("{}/v1/messages", self.base_url))
            .json(&json!({ "message": message }))
            .send()
            .await
            .map_err(|e| InstanceError::Network(e.to_string()))?;
        Ok(())
    }
    
    fn status(&self) -> InstanceStatus {
        InstanceStatus::Running
    }
    
    async fn stop(&mut self) -> Result<(), InstanceError> {
        Ok(())
    }
}

impl OpencodeInstance {
    async fn sse_event_loop(&self) {
        let url = format!("{}/v1/events", self.base_url);
        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return,
        };
        
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if let Ok(bytes) = chunk {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(event) = serde_json::from_str::<AgentEvent>(data) {
                            self.event_sink.emit("agent.event", json!(event));
                        }
                    }
                }
            }
        }
    }
}
```

---

## 6. AgentService 多实例管理

### 6.1 核心结构

```rust
pub struct AgentService {
    plugins: HashMap<String, Box<dyn AgentPlugin>>,
    instances: HashMap<String, AgentInstanceHandle>,
    event_sink: Arc<dyn EventSink>,
}

pub struct AgentInstanceHandle {
    id: String,
    plugin_type: String,
    instance: Box<dyn AgentInstance>,
    status: InstanceStatus,
}

impl AgentService {
    pub fn register_plugin(&mut self, plugin: Box<dyn AgentPlugin>);
    pub async fn create_instance(&self, plugin_type: &str, config: InstanceConfig) -> Result<String, Error>;
    pub async fn send_message(&self, instance_id: &str, message: &str) -> Result<(), Error>;
    pub fn get_instance_status(&self, instance_id: &str) -> Option<InstanceStatus>;
    pub async fn stop_instance(&self, instance_id: &str) -> Result<(), Error>;
    pub fn list_instances(&self) -> Vec<InstanceInfo>;
}
```

### 6.2 多实例支持

```bash
dh gatewayd start opencode claudecode
# AgentService 创建两个实例：
# - instance-1: opencode (plugin_type="opencode")
# - instance-2: claudecode (plugin_type="claudecode")
```

---

## 7. CLI 指令设计

### 7.1 向后兼容

```bash
# 原有命令（不变）
dh gatewayd start
dh gatewayd start --daemon
dh gatewayd stop
dh gatewayd status

# 新增命令
dh gatewayd start opencode
dh gatewayd start opencode claudecode
dh gatewayd start codex --daemon
```

### 7.2 命令定义

```rust
pub enum GatewaydCommands {
    Start {
        #[arg(long)]
        daemon: bool,
        /// Agent types to auto-start (optional)
        agent_types: Vec<String>,
    },
    // ... 其他命令
}
```

### 7.3 启动流程

```
dh gatewayd start opencode
        │
        ▼
1. 启动 gatewayd（端口 2345/2346）
        │
        ▼
2. 注册 OpencodePlugin 到 AgentService
        │
        ▼
3. 创建 OpenCode 实例（启动 opencode serve）
        │
        ▼
4. 打印状态信息
```

---

## 8. 错误处理

| 场景 | 处理策略 |
|------|---------|
| `opencode` 未安装 | CLI 报错提示安装 OpenCode CLI |
| `opencode serve` 端口冲突 | 自动寻找可用端口 |
| `opencode serve` 启动失败 | 返回错误，不创建实例 |
| SSE 连接断开 | 自动重连（指数退避） |
| 发送消息时实例未就绪 | 返回 503 Service Unavailable |
| 停止 gatewayd | 优雅停止所有 agent 实例 |

---

## 9. 测试策略

| 测试目标 | 方法 |
|---------|------|
| `WebSocketEventSink` | 单元测试：mock SessionManager，验证广播消息格式 |
| `OpencodePlugin::create_instance` | mock HTTP server，验证启动流程 |
| `AgentService` 多实例 | 单元测试：mock plugin，验证注册/创建/停止 |
| `EventSink` trait 解耦 | 验证 agent-core 编译时不依赖 tauri |
| WebSocket handler 路由 | 集成测试：发送 agent.createInstance，验证实例创建 |
| CLI `start opencode` | 手动测试：运行命令，验证输出 |

---

## 10. 验收标准

### Phase 1
1. [ ] `agent-core` 和 `opencode-plugin` 成功迁移到 `crates/`
2. [ ] `agent-core` 编译时不依赖 `tauri`
3. [ ] `OpencodeService` 已删除
4. [ ] `agent-runtime` 已删除
5. [ ] `cargo check --workspace` 0 errors

### Phase 2
6. [ ] `OpencodePlugin` 使用 `opencode serve` + HTTP/SSE（替代 MCP）
7. [ ] WebSocket handler 路由到 `AgentService`（替代 `OpencodeService`）
8. [ ] 前端通过 WebSocket 正常接收 `agent.event` / `session.log`
9. [ ] `AgentService` 支持创建多个并行的 agent 实例
10. [ ] `cargo test` 全部通过

---

## 11. 未来扩展点

- **Claude Code 插件**：复用同一套 HTTP/SSE 架构，替换为 `claude-code serve`
- **Codex 插件**：复用同一套架构，替换为 `codex serve`
- **Gatewayd 集成**：在 gatewayd 中初始化 AgentService，暴露 `POST /agents` 和 `POST /agents/{id}/message`
- **配置文件注册**：从 `~/.deepharness/agents.yaml` 动态加载插件配置
