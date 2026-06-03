# Agent Gateway 设计文档

> 日期：2026-06-01  
> 状态：已批准  
> 作者：AI Assistant

---

## 1. 概述

### 1.1 背景

当前 DeepHarness Desktop 应用拥有完整的 UI 骨架（会话列表、聊天面板、步骤渲染、权限询问、代码变更展示），但 `handleSendMessage` 和 `generateAIReply` 是纯本地 mock，没有任何真实的 AI 后端调用。本设计引入 **Agent Gateway** 层，将用户会话通过 Gateway 路由到对应的智能体服务。

### 1.2 目标

- 为每个智能体类型（opencode、claude-code 等）提供独立的适配包
- 首先实现 **OpenCode** 适配：模式切换、智能会话、指令接收、信息流返回
- 使用 **sidecar 模式**启动和管理本地智能体进程
- 最多允许同时运行 **6 个智能体实例**
- 每个智能体实例拥有**独立的 SQLite 数据文件**

### 1.3 非目标

- 本阶段不实现 claude-code、cursor-agent、codex 的适配（只预留接口）
- 不实现真实 LLM 调用（由 OpenCode server 负责）
- 不修改现有 UI 组件的视觉样式

---

## 2. 架构设计

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                      前端 (React)                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ AgentPanel  │  │ ChatPanel   │  │ AgentGateway        │ │
│  │ (UI 状态)    │  │ (消息渲染)   │  │ (HTTP调用OpenCode)  │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
│         │                │                     │            │
│         └────────────────┴─────────────────────┘            │
│                          │                                  │
│                   Tauri Invoke / Event                      │
│                          │                                  │
└──────────────────────────┼──────────────────────────────────┘
                           │
┌──────────────────────────┼──────────────────────────────────┐
│                      Rust 后端                               │
│  ┌───────────────────────┴──────────────────────────────┐  │
│  │              SidecarManager (进程管理)                 │  │
│  │  - 启停 opencode serve 进程                           │  │
│  │  - 端口分配 (4000~4005)                               │  │
│  │  - 健康检查 / 自动重启                                │  │
│  │  - 最多6个实例限制                                    │  │
│  └───────────────────────┬──────────────────────────────┘  │
│                          │                                  │
│  ┌───────────────────────┴──────────────────────────────┐  │
│  │              AgentDbManager (数据隔离)                 │  │
│  │  - 每个智能体独立 SQLite 文件                         │  │
│  │  - 路径: app_data/agents/{instance_id}/data.db        │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                           │
                    opencode serve (sidecar)
                    HTTP + SSE on localhost:{port}
```

### 2.2 设计原则

- **混合 Gateway**：Rust 负责进程管理（启停、端口、健康检查），前端负责业务协议（HTTP 调用、SSE 解析、消息转换）
- **职责分离**：Rust 做它擅长的系统级工作，前端做它擅长的协议处理
- **动态发现**：前端不需要知道 sidecar 在哪个端口，只需要知道 "给 opencode-instance-abc123 发这条消息"

---

## 3. 组件设计

### 3.1 Rust 后端组件

| 组件 | 文件 | 职责 |
|------|------|------|
| `SidecarManager` | `src-tauri/src/sidecar_manager.rs` | 管理所有 opencode serve 进程。启动、停止、端口分配、健康检查、资源限制（最多6个） |
| `AgentDbManager` | `src-tauri/src/agent_db.rs` | 每个智能体独立的 SQLite 操作。创建/删除数据库，会话/消息/任务/文件变更的 CRUD |
| `Commands` | `src-tauri/src/commands.rs` | Tauri commands 暴露给前端调用 |

### 3.2 前端组件

| 组件 | 文件 | 职责 |
|------|------|------|
| `AgentGateway` | `src/agents/gateway.ts` | 封装 HTTP 调用 OpenCode server。处理 SSE 流式响应、消息协议转换 |
| `OpencodeAdapter` | `src/agents/opencode/adapter.ts` | OpenCode 专属适配。实现 AgentAdapter 接口，负责 OpenCode API 的具体调用 |
| `AgentAdapter` (interface) | `src/agents/types.ts` | 通用智能体接口定义 |
| `AgentManager` | `src/agents/manager.ts` | 前端状态管理。维护智能体实例列表、连接状态、消息路由 |
| `AgentRegistry` | `src/agents/registry.ts` | 智能体注册表。`opencode`, `claude-code` 等适配器的注册中心 |

### 3.3 核心接口定义

```typescript
// src/agents/types.ts

interface AgentAdapter {
  readonly agentKey: string;
  readonly displayName: string;
  
  /** 检测本地是否安装 */
  isInstalled(): Promise<boolean>;
  
  /** 启动 sidecar（通过 Tauri command 让 Rust 启动） */
  start(config: AgentStartConfig): Promise<void>;
  
  /** 停止 sidecar */
  stop(instanceId: string): Promise<void>;
  
  /** 发送消息，返回 SSE 流 */
  sendMessage(instanceId: string, message: string): AsyncIterable<AgentEvent>;
  
  /** 模式切换（build/plan） */
  setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void>;
  
  /** 获取当前状态 */
  getStatus(instanceId: string): Promise<AgentStatus>;
}

type AgentEvent = 
  | { type: 'thinking'; content: string }
  | { type: 'tool_use'; toolName: string; args: Record<string, unknown> }
  | { type: 'tool_result'; toolName: string; result: string; failed?: boolean }
  | { type: 'ask_permission'; toolName: string; message: string }
  | { type: 'ask_user'; questions: Question[] }
  | { type: 'text_delta'; content: string }
  | { type: 'done' }
  | { type: 'error'; message: string };

interface AgentStartConfig {
  instanceId: string;
  workspace: string;
  port?: number; // 不传则自动分配
}

type AgentStatus = 
  | { state: 'stopped' }
  | { state: 'starting' }
  | { state: 'running'; port: number; pid: number }
  | { state: 'crashed'; error?: string };
```

---

## 4. 数据流设计

### 4.1 启动智能体

```text
1. 用户点击 AgentPanel 中的智能体
2. AgentManager 调用 Tauri: start_agent({ 
     instanceId, 
     agentKey: 'opencode', 
     workspace: '/path/to/project',
     port: auto 
   })
3. Rust SidecarManager:
   a. 检查是否已有该 instanceId 的进程，有则复用
   b. 检查总实例数是否 >= 6，是则拒绝并报错
   c. 分配空闲端口（4000~4005）
   d. 执行 opencode serve --port {port} --cors "*"
   e. 等待进程就绪（轮询 /health 或等待 stdout）
   f. 返回 { port, pid, status: 'running' }
4. AgentManager 更新前端状态：该智能体为"已连接"
5. AgentDbManager 创建该智能体的 SQLite 文件（如尚未创建）
```

### 4.2 发送消息

```text
1. 用户在 ChatPanel 输入消息，点击发送
2. ChatPanel 调用 AgentGateway.sendMessage(instanceId, content)
3. AgentGateway 根据 agentKey 获取对应 Adapter（如 OpencodeAdapter）
4. OpencodeAdapter：
   a. 通过 Tauri 获取该 instance 的当前端口
   b. HTTP POST localhost:{port}/v1/messages
   c. 接收 SSE 流
   d. 解析 OpenCode 的事件格式，转换为通用 AgentEvent
5. ChatPanel 订阅 AgentEvent 流，实时渲染
6. 同时，AgentDbManager 持久化消息到该智能体的 SQLite
```

### 4.3 停止/删除智能体

```text
1. 用户删除智能体实例
2. AgentManager 调用 Tauri: stop_agent({ instanceId, deleteData: true })
3. Rust SidecarManager: kill 进程
4. Rust AgentDbManager: 删除目录 agents/{instanceId}/
5. AgentManager 从前端状态中移除该实例
```

---

## 5. 数据存储设计

### 5.1 存储策略

**每智能体实例一个独立的 `.db` 文件**，路径：`{app_data_dir}/agents/{instance_id}/data.db`

选择理由：
- 与"每智能体一套表"的隔离度相同，但管理更简单
- 删除智能体时直接删除一个文件，而不是 DROP 多张表
- Rust `rusqlite` 支持多连接
- 单独备份/导出单个智能体数据更容易

### 5.2 Schema 设计

```sql
-- 会话表
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    model TEXT,
    created_at TEXT,
    updated_at TEXT
);

-- 消息表（兼容现有 Message/MessageStep 结构）
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    steps TEXT,          -- JSON 序列化的 MessageStep[]
    is_complete INTEGER DEFAULT 0,
    token_in INTEGER,
    token_out INTEGER,
    duration_ms INTEGER,
    created_at TEXT
);

-- 任务表
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT
);

-- 文件变更表
CREATE TABLE modified_files (
    id TEXT PRIMARY KEY,
    conversation_id TEXT,
    file_path TEXT NOT NULL,
    change_type TEXT NOT NULL,
    diff TEXT,
    created_at TEXT
);

-- 元数据表
CREATE TABLE agent_meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
```

### 5.3 主库保留表

`profiles` 表仍保留在主 `app.db` 中（全局唯一），用户档案不随智能体实例变化。

---

## 6. OpenCode 适配设计

### 6.1 安装检测

```typescript
// OpencodeAdapter.isInstalled()
async isInstalled(): Promise<boolean> {
  try {
    const result = await invoke<string>('check_opencode_installed');
    return !!result;
  } catch {
    return false;
  }
}
```

未安装时前端弹出 Toast："当前智能体 OpenCode 尚未安装，请先运行: npm install -g opencode-ai"

### 6.2 模式切换

OpenCode 支持 `build`（全权限开发模式）和 `plan`（只读分析模式）两种模式。通过 Tab 键切换。

Gateway 层通过 OpenCode server 的 `/v1/agents/mode`（或对应接口）进行模式切换，前端在底部状态栏显示当前模式并提供切换按钮。

### 6.3 信息流返回（SSE）

OpenCode server 返回 SSE 流，OpencodeAdapter 负责解析并转换为通用 AgentEvent：

| OpenCode SSE 事件 | AgentEvent 转换 |
|------------------|----------------|
| `thinking` | `{ type: 'thinking', content }` |
| `tool_use` | `{ type: 'tool_use', toolName, args }` |
| `tool_result` | `{ type: 'tool_result', toolName, result, failed }` |
| `permission_request` | `{ type: 'ask_permission', toolName, message }` |
| `question` | `{ type: 'ask_user', questions }` |
| `content_delta` | `{ type: 'text_delta', content }` |
| `done` | `{ type: 'done' }` |
| `error` | `{ type: 'error', message }` |

---

## 7. 错误处理与边界情况

| 场景 | 处理策略 |
|------|----------|
| **OpenCode 未安装** | `isInstalled()` 返回 false，Toast 提示安装命令，禁用启动按钮 |
| **端口被占用** | 尝试 4000~4005，全部被占用时报错"无法分配端口" |
| **opencode serve 启动失败** | Rust 捕获 stderr，返回错误详情给前端 |
| **opencode serve 运行时崩溃** | 健康检查每 5s 轮询 /health，无响应标记为 `crashed`，前端显示"已断开"，提供"重新连接"按钮 |
| **SSE 流中断** | 捕获 AbortError/NetworkError，转换为 `AgentEvent { type: 'error' }`，前端可"重试" |
| **达到6个实例上限** | 弹窗提示"最多同时运行6个智能体，请先停止其他智能体" |
| **切换智能体时** | 当前智能体的 SSE 流保持不断（后台运行），用户可随时切回来继续对话 |
| **应用退出时** | Tauri `RunEvent::Exit` 监听，优雅关闭所有 sidecar（SIGTERM -> 等待3s -> SIGKILL） |

---

## 8. 文件结构

```
src/
├── agents/
│   ├── types.ts              # 通用接口定义
│   ├── registry.ts           # 智能体注册表
│   ├── manager.ts            # 前端状态管理
│   ├── gateway.ts            # HTTP 网关 + SSE 处理
│   ├── opencode/
│   │   ├── adapter.ts        # OpenCode 适配器实现
│   │   ├── types.ts          # OpenCode API 类型
│   │   └── parser.ts         # SSE 事件解析器
│   └── claude-code/          # 预留（未来实现）
│       └── .gitkeep
src-tauri/src/
├── main.rs                   # 入口，注册 commands
├── sidecar_manager.rs        # SidecarManager 实现
├── agent_db.rs               # AgentDbManager 实现
└── commands.rs               # Tauri commands
```

---

## 9. 验收标准

- [ ] 用户点击 OpenCode 智能体，如未安装则提示安装
- [ ] 已安装时，自动启动 `opencode serve` sidecar，显示"已连接"
- [ ] 用户发送消息，能看到真实的 AI 回复（通过 OpenCode server）
- [ ] 回复中包含 thinking、tool_use、tool_result 等步骤，正确渲染
- [ ] 权限询问时弹出对话框，用户可"本次同意/本Session同意/拒绝"
- [ ] 可切换 build/plan 模式
- [ ] 最多同时运行 6 个智能体实例
- [ ] 删除智能体时，进程被 kill，数据文件被清理
- [ ] 应用退出时，所有 sidecar 进程被优雅关闭

---

## 10. 附录：OpenCode Server API 参考

基于 OpenCode 官方文档 (`open-code.ai/en/docs/server`)：

- `opencode serve` 启动 headless HTTP server
- 默认随机端口，可通过 `--port` 指定
- OpenAPI 3.1 规范可在 `http://localhost:{port}/doc` 查看
- 支持 HTTP basic auth（`OPENCODE_SERVER_PASSWORD`）
- 核心 API 端点：Sessions、Messages、Commands、Files、Tools、Agents、Events

> **注意**：OpenCode API 可能随版本迭代，前端 Adapter 应保持灵活，通过 OpenAPI spec 动态发现可用接口。
