# HTTP + SSE 流式架构设计

## 概述

将前后端通信从 WebSocket 改为 HTTP + SSE，实现真正的 AI 流式输出。

## 背景

当前架构使用 WebSocket JSON-RPC，但 `opencode run` 是阻塞式命令，无法流式推送。需要改为 SSE（Server-Sent Events）实现实时流式输出。

## 架构设计

### 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                        浏览器前端                             │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  HTTP Client │  │  SSE Client  │  │   Chat Store     │  │
│  │  (DB操作)    │  │  (AI流式)    │  │  (消息状态管理)   │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────────────┘  │
│         │                 │                                   │
│         └─────────────────┘                                   │
│                   │                                           │
│              HTTP/SSE                                        │
│                   │                                           │
├───────────────────┼─────────────────────────────────────────┤
│                   │              Rust 后端                    │
│         ┌─────────┴──────────┐                               │
│         │   HTTP Server      │                               │
│         │   (Axum)           │                               │
│         └─────────┬──────────┘                               │
│                   │                                           │
│    ┌──────────────┼──────────────┐                           │
│    │              │              │                            │
│ ┌──┴───┐    ┌────┴────┐   ┌────┴────┐                      │
│ │DB API│    │Agent API│   │SSE Endpoint│                    │
│ └──┬───┘    └────┬────┘   └────┬────┘                      │
│    │             │             │                              │
│ ┌──┴───┐    ┌────┴────┐   ┌──┴─────┐                       │
│ │DB Svc│    │OpenCode │   │Session │                       │
│ │      │    │  Svc    │   │Manager │                       │
│ └──────┘    └─────────┘   └───────┘                       │
└─────────────────────────────────────────────────────────────┘
```

### 组件说明

#### 1. HTTP Server (Axum)
- 监听固定端口（如 9527）
- 提供 RESTful API
- SSE endpoint 支持

#### 2. DB API
- 处理所有数据库操作
- 认证（登录/注册）
- 会话管理（CRUD）
- 消息存储

#### 3. Agent API
- 启动 AI 任务
- 管理 opencode 进程

#### 4. SSE Endpoint
- 每个会话一个 SSE 连接
- 推送 AI 流式输出
- 事件类型：thinking, token, done, error

#### 5. Session Manager
- 管理活跃的 SSE 连接
- 关联会话 ID 和 SSE sender
- 清理断开的连接

## API 设计

### HTTP Endpoints

#### 认证
```
POST /api/auth/signup
Body: { username: string, password: string }
Response: { user: AuthUser, error: null }

POST /api/auth/signin
Body: { username: string, password: string }
Response: { user: AuthUser, error: null }

GET /api/profile/:userId
Response: Profile
```

#### 会话
```
GET /api/conversations?userId=xxx&limit=50
Response: Conversation[]

POST /api/conversations
Body: { user_id, title, agent, model }
Response: Conversation

PUT /api/conversations/:id
Body: Partial<Conversation>
Response: void

DELETE /api/conversations/:id
Response: void
```

#### 消息
```
GET /api/messages?conversationId=xxx&limit=100
Response: Message[]

POST /api/messages
Body: { conversation_id, role, content }
Response: Message
```

#### Agent
```
POST /api/agent/run
Body: { message: string, sessionId?: string }
Response: { taskId: string, status: "started" }
```

### SSE Endpoint

```
GET /api/sse/:conversationId
Headers: Accept: text/event-stream
```

#### SSE 事件格式

```
event: thinking
data: {"content": "AI 正在思考..."}

event: token
data: {"content": "你", "index": 0}

event: token
data: {"content": "好", "index": 1}

event: done
data: {"sessionID": "ses_xxx"}

event: error
data: {"message": "错误信息"}
```

## 流式流程

### 用户发送消息

1. **创建用户消息**
   ```
   POST /api/messages
   Body: { conversation_id, role: "user", content }
   ```

2. **启动 AI 任务**
   ```
   POST /api/agent/run
   Body: { message, sessionId }
   Response: { taskId, status: "started" }
   ```

3. **建立 SSE 连接**
   ```
   GET /api/sse/:conversationId
   ```

4. **后端执行 opencode run**
   - 实时读取 stdout 的 JSON Lines
   - 解析每个事件
   - 通过 SSE 推送到前端

5. **前端展示**
   - 收到 `thinking` → 显示"AI 正在思考..." + isTyping（三个点）
   - 收到第一个 `token` → isTyping 消失，开始显示文字
   - 收到后续 `token` → 追加文字
   - 收到 `done` → 状态变为"已完成"
   - 收到 `error` → 显示错误信息

### 切换会话

1. **关闭当前 SSE**
   - 调用 `eventSource.close()`

2. **建立新 SSE**
   - 创建新的 `EventSource`
   - 连接到新会话的 SSE endpoint

## 数据模型

### 前端状态 (Zustand)

```typescript
interface ChatState {
  conversations: Conversation[];
  currentConversationId: string | null;
  opencodeSessionId: string | null;
  messages: Message[];
  isStreaming: boolean;
  isTyping: boolean;  // 新增：显示三个点
  activeInstanceId: string | null;
  
  sendMessage: (content: string) => Promise<void>;
  appendToken: (token: string) => void;  // 新增：追加 token
  setIsTyping: (isTyping: boolean) => void;  // 新增
  setIsStreaming: (isStreaming: boolean) => void;
}
```

### SSE 事件类型

```typescript
type SSEEvent =
  | { type: 'thinking'; content: string }
  | { type: 'token'; content: string; index: number }
  | { type: 'done'; sessionID: string }
  | { type: 'error'; message: string };
```

## 错误处理

### HTTP 错误
- 4xx: 客户端错误（参数错误、未授权等）
- 5xx: 服务端错误（数据库错误、opencode 进程错误等）

### SSE 错误
- 连接断开：前端自动重连（最多 3 次）
- 超时：30 秒无数据自动关闭
- opencode 错误：推送 `error` 事件

## 实现计划

### Phase 1: 后端 HTTP + SSE 服务
1. 添加 Axum 依赖
2. 创建 HTTP router
3. 实现 DB API handlers
4. 实现 SSE endpoint
5. 集成 opencode 流式读取

### Phase 2: 前端 HTTP Client
1. 创建 `src/services/http-client.ts`
2. 实现认证 API
3. 实现会话/消息 API
4. 替换 `ws-client.ts`

### Phase 3: 前端 SSE Client
1. 创建 `src/services/sse-client.ts`
2. 实现 SSE 连接管理
3. 处理 SSE 事件
4. 切换会话时重建 SSE

### Phase 4: UI 流式展示
1. 修改 `ChatPanel.tsx`
2. 实现 isTyping 状态
3. 实现 token 追加展示
4. 测试流式效果

## 风险评估

### 技术风险
- **Axum 集成**: Tauri 中集成 Axum 需要验证兼容性
- **SSE 稳定性**: 浏览器对 SSE 的支持良好，但断线重连需要测试
- **opencode 进程管理**: 多个并发请求需要管理多个 opencode 进程

### 缓解措施
- 保留现有 WebSocket 作为 fallback（Phase 1 中同时运行）
- 充分测试 SSE 断线重连
- 限制并发 opencode 进程数量

## 成功标准

1. ✅ AI 回复逐字/逐行显示（流式）
2. ✅ 只有一个 AI 消息（无重复）
3. ✅ isTyping 在第一个 token 到达后消失
4. ✅ 切换会话时 SSE 正确关闭/重建
5. ✅ 所有 DB 操作通过 HTTP API
