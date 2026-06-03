# WebSocket + MCP Gateway 协议重新设计

> 日期：2026-06-03
> 状态：待审批
> 作者：AI Assistant
> 目标：替换不稳定的 CLI spawn + stdout 解析，采用标准 MCP 协议和 WebSocket 双向通信

---

## 1. 概述

### 1.1 背景

当前架构存在以下问题：
- **Rust 后端 ↔ OpenCode**：通过 `opencode run --format json` spawn CLI 进程，解析 stdout 的 JSON lines。这种方式脆弱，进程管理复杂，解析逻辑容易出错。
- **前端 ↔ Rust 后端**：通过 Tauri Commands (`invoke`) 和 Tauri Events (`emit`/`listen`) 通信。这是请求-响应模型，不适合流式事件推送，且与 Tauri 强耦合。

### 1.2 目标

1. **前端 ↔ Rust Gateway**：使用 **WebSocket + JSON-RPC 2.0**，完全替代 Tauri IPC
2. **Rust Gateway ↔ Agent**：使用 **MCP (Model Context Protocol) stdio**，替代自定义 JSON lines 解析
3. **Rust Gateway 双重角色**：WebSocket Server（对前端）+ MCP Client（对 Agent）
4. **前端状态管理**：迁移到 Zustand，替换现有分散的 useState/useEffect
5. **页面 UI 保持不变**：仅替换底层通信和状态管理

### 1.3 非目标

- 不修改页面视觉样式和交互流程
- 不增加新的 Agent 类型（只适配现有的 OpenCode，为 Claude Code 预留接口）
- 不实现 MCP HTTP transport（仅 stdio）
- 不改变数据库存储结构

---

## 2. 整体架构

### 2.1 三层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    前端 (React + TypeScript)                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ Workspace   │  │ ChatPanel   │  │ Left/Right Panel    │ │
│  │ (页面布局)   │  │ (消息渲染)   │  │ (文件/任务/会话)     │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
│         │                │                     │            │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  State Layer (Zustand)                                │  │
│  │  - agentStore: 实例列表、连接状态                      │  │
│  │  - chatStore: 消息、会话、事件流                       │  │
│  │  - logStore: Session Logs                            │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  WebSocket Client (JSON-RPC 2.0)                      │  │
│  │  - 连接管理、心跳、自动重连                            │  │
│  │  - 请求-响应匹配 (id 映射)                             │  │
│  │  - 事件订阅/分发                                       │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────┼───────────────────────────────────────────────────┘
          │ WebSocket (ws://localhost:{port})
┌─────────┼───────────────────────────────────────────────────┐
│  Rust 后端 (Tauri App)                                      │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  WebSocket Server (tokio-tungstenite)                 │  │
│  │  - 监听动态端口                                        │  │
│  │  - 连接生命周期管理                                    │  │
│  │  - 启动后通过 Tauri IPC 告知前端地址                   │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Gateway Router (JSON-RPC 2.0 Dispatcher)             │  │
│  │  - 方法路由: agent.createInstance, agent.sendMessage  │  │
│  │  - 通知转发: agent:event → WebSocket broadcast        │  │
│  │  - 会话管理: 连接 ↔ 实例 的映射                        │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  AgentService (保持现有，适配新协议)                   │  │
│  │  - PluginRegistry / InstanceRegistry                   │  │
│  │  - 每个实例持有 McpClient                              │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  McpClient (每个 AgentInstance 一个)                   │  │
│  │  - stdio transport: spawn process + stdin/stdout       │  │
│  │  - JSON-RPC 2.0 序列化/反序列化                        │  │
│  │  - MCP 能力协商 (initialize, tools/list, etc.)         │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────┼───────────────────────────────────────────────────┘
          │ stdio (stdin/stdout JSON-RPC 2.0)
┌─────────┼───────────────────────────────────────────────────┐
│  Agent 进程 (OpenCode / Claude Code MCP Server)             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  MCP Server                                           │  │
│  │  - tools/call: 执行工具                                │  │
│  │  - resources/read: 读取资源                            │  │
│  │  - prompts/get: 获取提示词                             │  │
│  │  - 事件推送 (notification)                             │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 前端状态管理 | Zustand | 轻量、TypeScript 友好、支持异步流、替代现有分散的 useState |
| WebSocket 库 | 原生 `WebSocket` API + 自定义封装 | 不需要额外库，完全控制协议细节 |
| Rust WebSocket | `tokio-tungstenite` | Tokio 生态、性能优秀、与现有 async runtime 兼容 |
| MCP Client | 自定义实现（基于 `serde_json` + `tokio::process`） | 当前没有完美适配的 Rust MCP 库，stdio transport 逻辑简单 |
| JSON-RPC 2.0 | 自定义实现 | 前后端都需要，逻辑可控 |

---

## 3. WebSocket 协议规范（JSON-RPC 2.0）

### 3.1 连接建立流程

```
1. Tauri 应用启动
2. Rust 后端启动 WebSocket Server（绑定到 127.0.0.1:0，随机端口）
3. Rust 通过 Tauri IPC 暴露一个 Command: get_websocket_url()
   → 返回 "ws://127.0.0.1:{port}"
4. 前端 React 初始化时调用 get_websocket_url() 获取地址
5. 前端建立 WebSocket 连接
6. 连接成功后发送 JSON-RPC initialize 请求（握手）
7. Gateway 返回 initialize 响应，通信正式开始
```

**保留的唯一 Tauri IPC：**
| Command | 用途 |
|---------|------|
| `get_websocket_url()` | 获取 WebSocket server 地址 |
| `get_app_version()` | 获取应用版本（可选保留）|
| 其他文件系统/原生 API | 非 Agent 相关功能（如导出日志、打开文件夹）|

### 3.2 JSON-RPC 2.0 方法定义

**请求（前端 → Gateway）：**

```typescript
// 创建 Agent 实例
{
  "jsonrpc": "2.0",
  "id": "req-1",
  "method": "agent.createInstance",
  "params": {
    "pluginKey": "opencode",
    "name": "OpenCode Project A",
    "workspace": "/home/user/project-a"
  }
}

// 发送消息
{
  "jsonrpc": "2.0",
  "id": "req-2",
  "method": "agent.sendMessage",
  "params": {
    "instanceId": "inst-abc123",
    "conversationId": "conv-xyz789",
    "message": "帮我写一个排序算法"
  }
}

// 停止实例
{
  "jsonrpc": "2.0",
  "id": "req-3",
  "method": "agent.stopInstance",
  "params": {
    "instanceId": "inst-abc123"
  }
}

// 列出所有实例
{
  "jsonrpc": "2.0",
  "id": "req-4",
  "method": "agent.listInstances",
  "params": {}
}

// 获取实例详情
{
  "jsonrpc": "2.0",
  "id": "req-5",
  "method": "agent.getInstance",
  "params": {
    "instanceId": "inst-abc123"
  }
}

// 切换模式（build/plan）
{
  "jsonrpc": "2.0",
  "id": "req-6",
  "method": "agent.setMode",
  "params": {
    "instanceId": "inst-abc123",
    "mode": "build"
  }
}
```

**响应（Gateway → 前端）：**

```typescript
// 成功响应
{
  "jsonrpc": "2.0",
  "id": "req-1",
  "result": {
    "instanceId": "inst-abc123",
    "status": "running",
    "pluginKey": "opencode",
    "name": "OpenCode Project A",
    "workspace": "/home/user/project-a",
    "createdAt": "2026-06-03T15:43:01Z"
  }
}

// 错误响应
{
  "jsonrpc": "2.0",
  "id": "req-2",
  "error": {
    "code": -32001,
    "message": "Instance not found",
    "data": { "instanceId": "inst-abc123" }
  }
}
```

**通知（Gateway → 前端，单向推送，无 id）：**

```typescript
// Agent 事件流（核心）
{
  "jsonrpc": "2.0",
  "method": "agent.event",
  "params": {
    "instanceId": "inst-abc123",
    "conversationId": "conv-xyz789",
    "event": {
      "type": "thinking",
      "content": "让我分析一下需求..."
    }
  }
}

// 状态变更
{
  "jsonrpc": "2.0",
  "method": "agent.statusChanged",
  "params": {
    "instanceId": "inst-abc123",
    "status": "running",
    "pid": 12345
  }
}

// Session Log
{
  "jsonrpc": "2.0",
  "method": "session.log",
  "params": {
    "conversationId": "conv-xyz789",
    "timestamp": "2026-06-03T15:43:01Z",
    "level": "info",
    "source": "opencode-plugin",
    "message": "process spawned",
    "payload": { "pid": 12345 }
  }
}

// 心跳（保持连接）
{
  "jsonrpc": "2.0",
  "method": "heartbeat",
  "params": { "timestamp": 1717424581 }
}
```

### 3.3 错误码定义

| 错误码 | 含义 | 场景 |
|--------|------|------|
| `-32700` | Parse error | JSON 解析失败 |
| `-32600` | Invalid Request | 不符合 JSON-RPC 2.0 格式 |
| `-32601` | Method not found | 方法不存在 |
| `-32602` | Invalid params | 参数错误或缺失 |
| `-32603` | Internal error | Gateway 内部错误 |
| `-32001` | Instance not found | 实例不存在 |
| `-32002` | Plugin not found | 插件类型不存在 |
| `-32003` | Instance limit exceeded | 超过 6 个实例上限 |
| `-32004` | Process spawn failed | 启动 Agent 进程失败 |
| `-32005` | MCP initialization failed | MCP 握手失败 |
| `-32006` | WebSocket not connected | 前端未连接 WebSocket |

---

## 4. 前端状态层（Zustand）

### 4.1 Store 设计

```typescript
// stores/websocketStore.ts
interface WebSocketState {
  url: string | null;
  status: 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'error';
  reconnectAttempts: number;
  
  connect: () => Promise<void>;
  disconnect: () => void;
  sendRequest: <T>(method: string, params?: unknown) => Promise<T>;
  subscribe: (method: string, handler: (params: unknown) => void) => () => void;
}

// stores/agentStore.ts
interface AgentState {
  instances: AgentInstance[];
  activeInstanceId: string | null;
  
  createInstance: (config: CreateInstanceConfig) => Promise<AgentInstance>;
  stopInstance: (id: string) => Promise<void>;
  setActiveInstance: (id: string | null) => void;
  updateInstanceStatus: (id: string, status: InstanceStatus) => void;
}

// stores/chatStore.ts
interface ChatState {
  conversations: Conversation[];
  currentConversationId: string | null;
  messages: Message[];
  isStreaming: boolean;
  
  sendMessage: (content: string) => Promise<void>;
  appendEvent: (event: AgentEvent) => void;
  loadConversation: (id: string) => Promise<void>;
}

// stores/logStore.ts
interface LogState {
  logs: SessionLogEntry[];
  filteredLogs: SessionLogEntry[];
  
  appendLog: (log: SessionLogEntry) => void;
  loadHistory: (conversationId: string) => Promise<void>;
  filterByLevel: (level: LogLevel) => void;
}
```

### 4.2 前端组件适配（保持 UI 不变）

现有组件只需要替换数据来源：

| 组件 | 当前 | 变更后 |
|------|------|--------|
| `WorkspacePage.tsx` | `useAgentService()` (Tauri invoke/listen) | `useAgentStore()`, `useChatStore()` |
| `ChatPanel.tsx` | `useState(messages)` + `listen('agent:event')` | `useChatStore()` 的 `messages` 和 `appendEvent` |
| `LeftPanel.tsx` | `useState(instances)` | `useAgentStore()` 的 `instances` |
| `SessionLogDrawer.tsx` | `useSessionLogRust()` | `useLogStore()` |

---

## 5. Rust Gateway 架构

### 5.1 Rust 模块结构

```
src-tauri/
  Cargo.toml                    # workspace 根（新增 gateway crate）
  src/
    main.rs                     # 初始化 WebSocket Server + AgentService + SessionLogger
    lib.rs                      # 模块聚合
    commands/
      mod.rs                    # 保留少量 Tauri commands
      system.rs                 # get_websocket_url, get_app_version
    gateway/                    # 新增：WebSocket Gateway 核心
      mod.rs
      server.rs                 # WebSocket Server（tokio-tungstenite）
      router.rs                 # JSON-RPC 2.0 路由分发
      connection.rs             # 连接管理（心跳、自动断开）
      codec.rs                  # JSON-RPC 序列化/反序列化
      handlers/                 # 方法处理器
        agent.rs                # agent.* 方法
        session.rs              # session.* 方法
    mcp/                        # 新增：MCP Client 实现
      mod.rs
      client.rs                 # McpClient 结构
      transport.rs              # stdio transport（spawn + stdin/stdout）
      protocol.rs               # MCP JSON-RPC 方法封装
      types.rs                  # MCP 类型定义（Initialize, Tool, Resource 等）
    service/                    # 现有，适配新接口
      agent_service.rs          # AgentService：PluginRegistry + InstanceRegistry
      plugin_registry.rs
      instance_registry.rs
    models/                     # 现有
      agent.rs
      event.rs
      log.rs
  crates/
    agent-core/                 # 现有，trait 调整
      src/
        plugin.rs               # AgentPlugin trait
        instance.rs             # AgentInstance trait（新增 MCP 相关方法）
        event.rs                # AgentEvent（保持不变）
        logger.rs               # SessionLogger（保持不变）
        error.rs                # 新增 GatewayError, McpError
    agent-runtime/              # 现有，废弃或保留用于进程管理
      src/
        process.rs              # 保留：ProcessHandle, spawn, kill
    opencode-plugin/            # 现有，适配 MCP
      src/
        plugin.rs               # OpencodePlugin 实现 AgentPlugin
        instance.rs             # OpencodeInstance 实现 AgentInstance
        mcp_adapter.rs          # 新增：将 AgentInstance 操作映射为 MCP 调用
```

### 5.2 核心 Rust 类型

```rust
// gateway/server.rs
pub struct WebSocketServer {
    addr: SocketAddr,
    agent_service: Arc<AgentService>,
    logger: Arc<SessionLogger>,
}

impl WebSocketServer {
    pub async fn start(
        &self,
        shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

// gateway/router.rs
pub struct GatewayRouter {
    agent_service: Arc<AgentService>,
    logger: Arc<SessionLogger>,
}

impl GatewayRouter {
    pub async fn handle_request(
        &self,
        conn_id: &str,
        req: JsonRpcRequest,
    ) -> JsonRpcResponse;
    
    pub async fn broadcast_notification(
        &self,
        method: &str,
        params: Value,
    );
}

// mcp/client.rs
pub struct McpClient {
    process: ProcessHandle,
    request_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(Value) + Send>>>>,
}

impl McpClient {
    pub async fn spawn(command: &str, args: &[String], workspace: &str) -> Result<Self, McpError>;
    
    pub async fn initialize(&self) -> Result<InitializeResult, McpError>;
    
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<ToolResult, McpError>;
    
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), McpError>;
    
    pub fn on_notification<F>(&self, method: &str, handler: F) 
    where F: Fn(Value) + Send + 'static;
}
```

### 5.3 AgentInstance trait 调整

```rust
// agent-core/src/instance.rs
pub trait AgentInstance: Send + Sync {
    fn id(&self) -> &str;
    fn status(&self) -> InstanceStatus;
    fn plugin_key(&self) -> &'static str;
    
    // 原有方法保留，但内部改为 MCP 通信
    fn send_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;
    
    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>>;
    
    // 新增：MCP 相关
    fn mcp_client(&self) -> Option<&McpClient>;
}
```

### 5.4 OpencodeInstance MCP 适配示例

```rust
impl AgentInstance for OpencodeInstance {
    fn send_message(&self, conversation_id: &str, message: &str) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let conversation_id = conversation_id.to_string();
        let message = message.to_string();
        Box::pin(async move {
            // 通过 MCP call_tool 调用 opencode 的 send_message 工具
            self.mcp_client.call_tool("send_message", json!({
                "conversation_id": conversation_id,
                "message": message
            })).await.map_err(|e| InstanceError::McpError(e))?;
            Ok(())
        })
    }
    
    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            self.mcp_client.shutdown().await.ok();
            Ok(())
        })
    }
}
```

---

## 6. 完整数据流

### 6.1 用户发送消息（端到端）

```
1. 前端 ChatPanel
   用户输入 "帮我写一个排序算法"，点击发送
   │
   ▼
2. chatStore.sendMessage(content)
   添加用户消息到本地状态
   │
   ▼
3. websocketStore.sendRequest("agent.sendMessage", {
      instanceId: "inst-abc123",
      conversationId: "conv-xyz789",
      message: "帮我写一个排序算法"
   })
   生成 JSON-RPC 请求，发送 WebSocket message
   │
   ▼
4. Rust WebSocket Server 接收
   解析 JSON → JsonRpcRequest
   │
   ▼
5. GatewayRouter 路由到 agent.sendMessage handler
   │
   ▼
6. AgentService::send_message(instance_id, message)
   查找 InstanceRegistry
   │
   ▼
7. OpencodeInstance::send_message(conversation_id, message)
   内部调用 McpClient::call_tool("send_message", ...)
   │
   ▼
8. McpClient 通过 stdin 发送 JSON-RPC
   {
     "jsonrpc": "2.0",
     "id": 1,
     "method": "tools/call",
     "params": {
       "name": "send_message",
       "arguments": {
         "conversation_id": "conv-xyz789",
         "message": "帮我写一个排序算法"
       }
     }
   }
   │
   ▼
9. OpenCode MCP Server 处理
   执行 LLM 调用，产生流式输出
   │
   ▼
10. OpenCode 通过 stdout 推送通知
    {
      "jsonrpc": "2.0",
      "method": "notifications/message",
      "params": {
        "conversation_id": "conv-xyz789",
        "delta": { "type": "thinking", "content": "让我分析..." }
      }
    }
    │
    ▼
11. McpClient 通知处理器触发
    解析 notification → AgentEvent::Thinking
    emit_event("agent:event", { instance_id, event })
    │
    ▼
12. GatewayRouter 的 broadcast_notification
    将通知转发给所有连接的 WebSocket clients
    │
    ▼
13. 前端 WebSocket onmessage
    解析 JSON-RPC notification
    │
    ▼
14. websocketStore 分发到订阅者
    chatStore.appendEvent(event) 被调用
    │
    ▼
15. ChatPanel 重新渲染
    显示 thinking 内容
    │
    ...（后续 deltas 重复 10-15）
    │
    ▼
16. OpenCode 发送 done notification
    │
    ▼
17. 前端收到 done event
    chatStore.setIsStreaming(false)
    ChatPanel 显示完成状态
```

### 6.2 连接断开与重连

```
前端检测到 WebSocket onclose
  │
  ▼
自动重连策略（指数退避）
  - 第1次：立即重连
  - 第2次：等待 1s
  - 第3次：等待 2s
  - 第4次：等待 4s
  - 最大间隔：30s
  - 最大重试次数：无限（用户可手动停止）
  │
  ▼
重连成功后
  1. 前端发送 agent.listInstances 恢复实例列表
  2. 前端发送 agent.getInstance 恢复每个实例状态
  3. 对于 running 的实例，重新订阅 agent.event 通知
  4. 如果有未完成的对话，chatStore 恢复消息状态
```

---

## 7. 错误处理与边界情况

| 场景 | 处理策略 |
|------|----------|
| **WebSocket 连接断开** | 前端自动重连（指数退避），重连后恢复状态 |
| **Gateway 崩溃/重启** | WebSocket 断开，前端重连，重新获取实例列表 |
| **Agent 进程崩溃** | McpClient stdout EOF，McpClient 上报 error notification → Gateway broadcast error event → 前端显示"Agent 已断开" |
| **MCP initialize 失败** | agent.createInstance 返回错误，前端显示具体错误信息 |
| **MCP call_tool 超时** | 30s 超时，返回 error，前端显示超时提示 |
| **Agent 未安装** | Plugin::is_installed() 检查，createInstance 返回 PluginNotInstalled 错误 |
| **超过 6 个实例** | createInstance 返回 InstanceLimitExceeded 错误 |
| **并发消息发送** | Gateway 对每个实例的消息队列串行处理，避免 stdin 混叠 |
| **非法 JSON-RPC** | Gateway 返回 ParseError (-32700)，前端丢弃并记录日志 |

---

## 8. 迁移步骤与实现计划

### 8.1 Phase 1：Rust Gateway 骨架（WebSocket Server + JSON-RPC 路由）
- 新增 `src-tauri/src/gateway/` 目录
- 实现 `WebSocketServer`（tokio-tungstenite）
- 实现 `GatewayRouter`（JSON-RPC 2.0 解析和路由）
- 实现 `JsonRpcCodec`（序列化/反序列化）
- 保留现有 Tauri Commands，新增 `get_websocket_url()`
- 前端新增 `src/stores/websocketStore.ts`
- **验证**：WebSocket 连接建立，心跳正常，Tauri IPC 仍可用

### 8.2 Phase 2：MCP Client 实现
- 新增 `src-tauri/src/mcp/` 目录
- 实现 `McpClient`（stdio transport）
- 实现 MCP 握手（initialize）
- 实现 `call_tool`, `read_resource`, `get_prompt`
- 实现通知处理器注册
- **验证**：用测试 MCP server（如一个简单的 echo server）验证 MCP 通信

### 8.3 Phase 3：AgentInstance MCP 适配
- 修改 `agent-core/src/instance.rs`（新增 MCP 相关方法）
- 修改 `opencode-plugin/src/instance.rs`（使用 McpClient）
- 废弃 `agent-runtime/src/process.rs` 中的自定义 JSON line 解析逻辑
- **验证**：`agent.createInstance` → MCP initialize → `agent.sendMessage` → MCP call_tool → 接收 notification

### 8.4 Phase 4：前端状态层迁移（Zustand）
- 新增 `src/stores/agentStore.ts`
- 新增 `src/stores/chatStore.ts`
- 新增 `src/stores/logStore.ts`
- 修改 `WorkspacePage.tsx`：替换 useAgentService → useAgentStore + useChatStore
- 修改 `ChatPanel.tsx`：替换 listen('agent:event') → chatStore.subscribe
- 修改 `LeftPanel.tsx`：替换状态管理 → agentStore
- 修改 `SessionLogDrawer.tsx`：替换 useSessionLogRust → logStore
- **验证**：前端 UI 保持不变，所有通信走 WebSocket

### 8.5 Phase 5：清理与废弃
- 删除 `src/agents/` 目录（adapter、parser、gateway、manager、registry）
- 删除 `src/hooks/use-agent-service.ts`
- 删除 `src/hooks/use-session-log-rust.ts`
- 删除 `src-tauri/src/commands/agent.rs`（或保留为薄层转发到 WebSocket）
- 删除 `src-tauri/src/sidecar_manager.rs`
- 删除 `src-tauri/src/agent_db.rs`（如果功能已合并）
- **验证**：`pnpm lint` 通过，无引用残留

### 8.6 Phase 6：端到端验证
- 启动应用，WebSocket 连接成功
- 创建 OpenCode 实例，MCP initialize 成功
- 发送消息，接收流式事件
- 验证 Session Logs 双写
- 验证断开重连
- 验证应用退出时优雅关闭

### 8.2 文件变更清单

| 操作 | 文件/目录 | 说明 |
|------|-----------|------|
| 新增 | `src/stores/websocketStore.ts` | WebSocket 连接管理 |
| 新增 | `src/stores/agentStore.ts` | Agent 状态管理 |
| 新增 | `src/stores/chatStore.ts` | 聊天状态管理 |
| 新增 | `src/stores/logStore.ts` | 日志状态管理 |
| 新增 | `src-tauri/src/gateway/` | WebSocket Gateway |
| 新增 | `src-tauri/src/mcp/` | MCP Client |
| 新增 | `src-tauri/src/commands/system.rs` | Tauri 系统 commands |
| 修改 | `src-tauri/crates/agent-core/src/instance.rs` | 新增 MCP 方法 |
| 修改 | `src-tauri/crates/opencode-plugin/src/instance.rs` | 使用 McpClient |
| 修改 | `src-tauri/src/main.rs` | 初始化 WebSocket Server |
| 修改 | `src-tauri/src/service/agent_service.rs` | 适配新接口 |
| 修改 | `src/pages/WorkspacePage.tsx` | 替换状态层 |
| 修改 | `src/components/workspace/ChatPanel.tsx` | 替换事件监听 |
| 修改 | `src/components/workspace/LeftPanel.tsx` | 替换状态 |
| 修改 | `src/components/workspace/SessionLogDrawer.tsx` | 替换日志 hook |
| 删除 | `src/agents/` | 废弃 |
| 删除 | `src/hooks/use-agent-service.ts` | 废弃 |
| 删除 | `src/hooks/use-session-log-rust.ts` | 废弃 |
| 删除 | `src-tauri/src/commands/agent.rs` | 废弃 |
| 删除 | `src-tauri/src/sidecar_manager.rs` | 废弃 |

---

## 9. 风险与应对措施

| 风险 | 影响 | 应对措施 |
|------|------|----------|
| **OpenCode MCP 支持不完善** | 高 | Phase 2 先用 echo MCP server 验证；如 OpenCode MCP 不成熟，可 fallback 到现有 CLI spawn + MCP 协议格式 |
| **WebSocket 端口冲突** | 中 | 绑定 `127.0.0.1:0`（随机端口），通过 Tauri IPC 告知前端 |
| **Tauri 与 WebSocket 共存复杂** | 中 | 保留 Tauri 用于应用框架，仅替换 IPC 层；Tauri Commands 减少到最少 |
| **前端状态迁移 Bug** | 中 | Phase 4 逐步替换，每改一个组件立即验证；保留旧代码作为参考直到全部完成 |
| **MCP stdio 性能瓶颈** | 低 | 实际测试后再优化；如需性能提升可后续增加 HTTP transport 支持 |
| **多窗口 WebSocket 连接** | 低 | 每个窗口独立 WebSocket 连接，Gateway 支持多连接；状态由 Gateway 统一管理 |

---

## 10. 验收标准

- [ ] WebSocket Server 启动成功，前端通过 `get_websocket_url()` 获取地址并连接
- [ ] JSON-RPC 2.0 请求-响应正常（agent.createInstance, agent.sendMessage 等）
- [ ] JSON-RPC Notification 推送正常（agent.event, agent.statusChanged）
- [ ] MCP Client 与测试 MCP server 握手成功
- [ ] OpenCode 实例通过 MCP 创建，initialize 成功
- [ ] 发送消息通过 MCP call_tool，接收 notification 事件流
- [ ] 前端 ChatPanel 正确渲染 thinking/tool_use/text_delta/done 等事件
- [ ] WebSocket 断开后自动重连，恢复实例列表和状态
- [ ] Session Logs 正常双写（WebSocket 推送 + SQLite 存储）
- [ ] 应用退出时 WebSocket Server 和所有 MCP 进程优雅关闭
- [ ] 最多同时运行 6 个智能体实例
- [ ] `pnpm lint` 通过，无 TypeScript/Rust 编译错误
- [ ] `pnpm tauri build` 编译通过
