# OpenCode 交互支持设计文档

## 1. 架构概述

采用**响应解析驱动 + SSE 辅助**方案，以 `POST /message` 同步响应为主触发源，SSE `/event` 为辅处理异步状态更新。

```
┌─────────────┐     WebSocket      ┌─────────────┐     HTTP API       ┌─────────────────┐
│ 前端 React   │ ◄───────────────► │ Rust 后端    │ ◄───────────────► │ opencode serve  │
│             │   agent.question   │             │   POST /message   │   (端口 3001)    │
│             │   agent.permission │             │   GET /event      │                  │
│             │   agent.todowrite  │             │                   │                  │
│             │ ─────────────────> │             │                   │                  │
│             │   agent.respond    │             │                   │                  │
└─────────────┘                    └─────────────┘                   └─────────────────┘
```

## 2. 数据流详细设计

### 2.1 正常对话流

1. 前端发送 `agent.sendMessage`（WebSocket）
2. 后端调用 `POST /session/{id}/message`
3. 后端解析响应 `parts`，检查是否包含交互请求
4. 如果包含交互请求 → 推送 WebSocket 通知给前端
5. 如果不包含 → 返回正常文本结果

> ⚠️ **格式说明**：以下 `tool_use` part 格式是基于 opencode 源码分析（`packages/opencode/src/tool/*.ts`）的推测。实际实现时需根据 `opencode serve` 的真实响应格式调整解析逻辑。

### 2.2 交互检测策略

后端解析 `POST /message` 响应的 `parts` 数组：

```json
{
  "info": { "sessionID": "ses_xxx", ... },
  "parts": [
    { "type": "step-start", ... },
    { "type": "text", "text": "..." },
    { "type": "tool_use", "toolName": "question", "input": { "questions": [...] } },
    { "type": "step-finish", ... }
  ]
}
```

当检测到 `type: "tool_use"` 且 `toolName` 为 `"question"` / `"todowrite"` 时，提取 `input` 作为交互 payload，通过 WebSocket 推送。

### 2.3 SSE 辅助事件流

后端同时维护一个全局 SSE 监听器连接 `GET /event`：

```
data: {"id":"evt_xxx","type":"server.connected","properties":{}}
data: {"id":"evt_xxx","type":"session.updated","properties":{"sessionID":"ses_xxx",...}}
data: {"id":"evt_xxx","type":"server.heartbeat","properties":{}}
```

SSE 事件按 `sessionID` 路由到对应 WebSocket 会话，用于：
- `session.updated`：更新会话标题、token 消耗等元信息
- 其他事件：日志记录、调试

### 2.4 用户回答流

1. 前端展示交互 UI（Permission / Question / Todo）
2. 用户填写/选择答案
3. 前端发送 `agent.respond`（WebSocket），payload 包含：
   - `sessionID`
   - `interactionType`：`question` | `permission` | `todo`
   - `response`：用户回答内容
4. 后端将回答格式化为 opencode 消息格式，再次调用 `POST /session/{id}/message`
5. opencode 继续执行，返回后续结果

## 3. 后端 API 设计（Rust）

### 3.1 OpencodeService 扩展

```rust
impl OpencodeService {
    /// 发送消息并解析响应，检测交互请求
    pub async fn send_message_and_detect_interaction(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<OpencodeResponse, String>;

    /// 启动全局 SSE 监听器（后台任务）
    pub async fn start_event_listener(
        &self,
        event_sender: tokio::sync::mpsc::Sender<SseEvent>,
    );

    /// 发送用户回答
    pub async fn send_response(
        &self,
        session_id: &str,
        response: &InteractionResponse,
    ) -> Result<serde_json::Value, String>;
}
```

### 3.2 新增数据结构

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpencodeResponse {
    pub session_id: String,
    pub parts: Vec<OpencodePart>,
    pub interaction: Option<InteractionRequest>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionRequest {
    Question { questions: Vec<QuestionItem> },
    Permission { tool_name: String, action: String },
    TodoWrite { todos: Vec<TodoItem> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuestionItem {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multiple: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: String, // pending | in_progress | completed | cancelled
    pub priority: String, // high | medium | low
}
```

### 3.3 WebSocket 事件类型

新增通知类型：
- `agent.question` → 推送 question 交互请求
- `agent.permission` → 推送 permission 交互请求
- `agent.todowrite` → 推送 todo 列表更新

新增请求类型：
- `agent.respond` → 前端发送用户回答

## 4. 前端组件设计（React）

### 4.1 类型扩展

```typescript
// types/types.ts
export interface MessageStep {
  type: 'thinking' | 'tool_use' | 'tool_result' | 'ask_permission' | 'ask_user' | 'final' | 'compress' | 'retry';
  content: string;
  toolName?: string;
  questions?: AskQuestion[];
  permissionType?: string;
  failed?: boolean;
  summary?: ToolSummary;
  compressInfo?: { ... };
  diff?: string;
  // 新增：交互相关
  interaction?: InteractionPayload;
}

export interface InteractionPayload {
  type: 'question' | 'permission' | 'todowrite';
  questions?: QuestionItem[];
  toolName?: string;
  action?: string;
  todos?: TodoItem[];
}

export interface QuestionItem {
  question: string;
  header: string;
  options: { label: string; description: string }[];
  multiple: boolean;
}

export interface TodoItem {
  id: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  priority: 'high' | 'medium' | 'low';
}
```

### 4.2 ChatPanel 交互组件

#### PermissionStep（权限询问）

按 opencode 格式展示：
- 显示工具名称（如 `bash`、`edit`）
- 显示具体操作描述
- 三个按钮：
  - `once` → 仅本次同意（对应 opencode "once"）
  - `session` → 本 Session 同意（对应 opencode "always"）
  - `deny` → 不同意（对应 opencode "reject"）

#### UserQuestionsStep（用户确认）

按 opencode `question` 工具格式展示：
- 支持多题（`questions` 数组）
- 每题显示：`header`（短标签）+ `question`（完整问题）
- 选项展示：`label` + `description`
- 支持单选/多选（`multiple`）
- 支持自定义输入（opencode 默认启用 `custom`）
- 多个问题可以导航切换，最后统一提交

#### TodoWriteStep（任务列表）

在消息流中展示 todo 更新：
- 显示任务列表
- 支持优先级颜色（high=红色, medium=黄色, low=灰色）
- 状态标记（pending/进行中/已完成）

### 4.3 RightPanel Todo 列表

扩展 `Task` 类型或新增 `Todo` 类型，展示 AI 的 todo 列表：
- 从后端推送的 `agent.todowrite` 事件更新
- 支持优先级图标
- 实时更新状态

### 4.4 WebSocket 事件订阅

```typescript
// websocketStore 新增订阅
wsStore.subscribe('agent.question', handleQuestion);
wsStore.subscribe('agent.permission', handlePermission);
wsStore.subscribe('agent.todowrite', handleTodoUpdate);

// 前端回答发送
wsStore.sendRequest('agent.respond', {
  sessionId,
  interactionType: 'question',
  response: { answers: [...] }
});
```

## 5. 交互状态机

每个会话维护一个交互状态：

```
Idle ──send_message──> WaitingForResponse
                          │
                          ▼
                    InteractionRequired
                          │
            ┌─────────────┼─────────────┐
            ▼             ▼             ▼
      QuestionAsked  PermissionAsked  TodoUpdated
            │             │             │
            └─────────────┴─────────────┘
                          │
                          ▼
                    UserResponded ──send_response──> Idle
```

状态转换由后端管理，前端通过 WebSocket 事件感知状态变化。

## 6. 错误处理

| 场景 | 处理策略 |
|------|----------|
| opencode serve 未启动 | fallback 到现有逻辑，显示错误提示 |
| SSE 连接断开 | 自动重连（指数退避），前端显示连接状态 |
| 交互超时（如用户长时间未回答） | 前端显示超时提示，允许取消或重新发送 |
| opencode 返回未知 part 类型 | 记录日志，忽略未知类型，不影响其他功能 |

## 7. 测试策略

1. **单元测试**：
   - `parse_interaction_from_parts` 解析器（各种 part 组合）
   - SSE 事件解析器
   - 前端交互组件（Question/Permission/Todo）

2. **集成测试**：
   - 端到端消息发送 → 交互检测 → 用户回答 → 结果返回
   - SSE 事件路由到正确会话

3. **Mock 测试**：
   - 使用 mock opencode serve 响应测试交互检测逻辑
   - 使用 mock SSE 事件测试前端 UI 更新
