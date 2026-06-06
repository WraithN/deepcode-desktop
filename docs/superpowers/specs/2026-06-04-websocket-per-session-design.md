# 每会话 WebSocket + 流式设计

## 概述

保持 WebSocket 架构，但改为**每个会话一个 WebSocket 连接**。激活会话时建立 WebSocket，离开会话时关闭。后端实时推送 AI 流式输出。

## 架构设计

### 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                        浏览器前端                             │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐  ┌──────────────────┐                │
│  │  Session WS      │  │  Session WS      │  ...          │
│  │  (会话 1)        │  │  (会话 2)        │                │
│  └────────┬─────────┘  └────────┬─────────┘                │
│           │                     │                           │
│           └─────────────────────┘                           │
│                     │                                       │
│              WebSocket (每会话独立)                          │
│                     │                                       │
├─────────────────────┼───────────────────────────────────────┤
│                     │              Rust 后端                 │
│         ┌───────────┴──────────┐                            │
│         │   WS Server          │                            │
│         │   (按会话管理连接)    │                            │
│         └───────────┬──────────┘                            │
│                     │                                       │
│    ┌────────────────┼────────────────┐                     │
│    │                │                │                      │
│ ┌──┴───┐      ┌────┴────┐    ┌─────┴─────┐               │
│ │DB API│      │Session  │    │Stream     │               │
│ │(JSON)│      │Manager  │    │Push       │               │
│ └──┬───┘      └────┬────┘    └─────┬─────┘               │
│    │               │               │                       │
│ ┌──┴───┐      ┌────┴────┐    ┌────┴────┐                │
│ │DB Svc│      │OpenCode │    │WS Sender│                │
│ │      │      │  Svc    │    │(每会话)  │                │
│ └──────┘      └─────────┘    └─────────┘                │
└─────────────────────────────────────────────────────────────┘
```

### 关键设计点

1. **每会话独立 WebSocket** - 每个会话有自己的 WebSocket 连接
2. **会话生命周期管理** - 激活时建立，离开时关闭
3. **流式推送** - 后端实时读取 opencode stdout 并推送
4. **isTyping 状态** - 第一个 token 到达前显示，到达后消失

## WebSocket 协议

### 连接建立

```
ws://localhost:9527/ws/:conversationId

Headers:
  X-Session-ID: <conversationId>
```

### 请求格式（JSON-RPC）

```json
{
  "jsonrpc": "2.0",
  "id": "req-123",
  "method": "agent.run",
  "params": {
    "message": "你好",
    "sessionId": "ses_xxx"
  }
}
```

### 响应格式

**立即响应（确认收到）：**
```json
{
  "jsonrpc": "2.0",
  "id": "req-123",
  "result": { "status": "started" }
}
```

**流式推送（通知）：**
```json
{
  "jsonrpc": "2.0",
  "method": "agent.thinking",
  "params": { "content": "AI 正在思考..." }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "agent.token",
  "params": { "content": "你", "index": 0 }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "agent.token",
  "params": { "content": "好", "index": 1 }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "agent.done",
  "params": { "sessionID": "ses_xxx" }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "agent.error",
  "params": { "message": "错误信息" }
}
```

## 前端状态管理

### ChatStore 更新

```typescript
interface ChatState {
  conversations: Conversation[];
  currentConversationId: string | null;
  opencodeSessionId: string | null;
  messages: Message[];
  isStreaming: boolean;
  isTyping: boolean;  // 新增
  activeInstanceId: string | null;
  wsConnections: Map<string, WebSocket>;  // 新增：会话 -> WebSocket
  
  sendMessage: (content: string) => Promise<void>;
  connectSession: (conversationId: string) => void;  // 新增
  disconnectSession: (conversationId: string) => void;  // 新增
  appendToken: (token: string) => void;  // 新增
  setIsTyping: (isTyping: boolean) => void;  // 新增
}
```

### 流式展示逻辑

1. **用户发送消息**
   - 创建用户消息
   - 创建 AI 消息占位符（is_complete: false, isTyping: true）

2. **收到 thinking 通知**
   - 显示"AI 正在思考..."
   - isTyping = true

3. **收到第一个 token 通知**
   - isTyping = false
   - 开始显示文字

4. **收到后续 token 通知**
   - 追加文字

5. **收到 done 通知**
   - is_complete = true
   - 保存 sessionID

## 后端实现

### Session Manager

```rust
pub struct SessionManager {
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
}

impl SessionManager {
    pub async fn register(&self, session_id: String, handle: ConnectionHandle) {
        // 注册会话连接
    }
    
    pub async fn unregister(&self, session_id: &str) {
        // 注销会话连接
    }
    
    pub async fn send_to_session(&self, session_id: &str, msg: Message) {
        // 向指定会话发送消息
    }
}
```

### 流式推送实现

```rust
async fn handle_run_streaming(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    conversation_id: String,
    message: String,
    session_id: Option<String>,
) {
    // 1. 启动 opencode run
    let mut child = Command::new("opencode")
        .arg("run")
        .arg(&message)
        .arg("--format").arg("json")
        .arg("--attach").arg(&opencode_service.get_attach_url())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    
    // 2. 实时读取 stdout
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout).lines();
    
    // 3. 逐行解析并推送
    while let Some(line) = reader.next_line().await.unwrap() {
        if let Ok(event) = serde_json::from_str::<Value>(&line) {
            // 解析事件类型
            let method = match event.get("type").and_then(|v| v.as_str()) {
                Some("step_start") => "agent.thinking",
                Some("text") => "agent.token",
                Some("step_finish") => "agent.done",
                _ => continue,
            };
            
            // 推送到对应会话
            let notification = json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": event
            });
            
            session_manager.send_to_session(
                &conversation_id,
                Message::Text(notification.to_string())
            ).await;
        }
    }
}
```

## 实现步骤

### Phase 1: 后端改造

1. **修改 WebSocket Server**
   - 支持按会话路由 (`/ws/:conversationId`)
   - SessionManager 管理连接

2. **实现流式推送**
   - 实时读取 opencode stdout
   - 解析 JSON Lines
   - 按会话推送

3. **修改 agent.run handler**
   - 立即返回 started
   - 后台执行流式推送

### Phase 2: 前端改造

1. **修改 WebSocketStore**
   - 支持每会话连接
   - 管理多个 WebSocket

2. **修改 ChatStore**
   - 添加 isTyping 状态
   - 处理流式事件
   - 切换会话时重建连接

3. **修改 ChatPanel**
   - 显示 isTyping（三个点）
   - 第一个 token 到达后消失
   - 追加 token 展示

### Phase 3: 测试

1. **单会话测试**
   - 发送消息
   - 验证流式输出
   - 验证 isTyping 行为

2. **多会话测试**
   - 切换会话
   - 验证 WebSocket 重建
   - 验证无重复消息

3. **异常测试**
   - 断线重连
   - opencode 错误
   - 网络异常

## 成功标准

1. ✅ 每会话独立 WebSocket
2. ✅ 激活会话建立连接，离开关闭
3. ✅ AI 回复逐字/逐行显示（流式）
4. ✅ isTyping 在第一个 token 到达后消失
5. ✅ 只有一个 AI 消息（无重复）
6. ✅ 切换会话时正确重建连接
