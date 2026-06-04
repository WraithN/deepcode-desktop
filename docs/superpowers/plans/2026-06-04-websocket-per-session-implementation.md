# 每会话 WebSocket + 流式实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现每会话独立 WebSocket，支持 AI 流式输出，isTyping 在第一个 token 到达后消失

**Architecture:** 后端实时读取 opencode stdout 的 JSON Lines，通过会话专属 WebSocket 推送；前端管理每会话连接，处理流式事件更新 UI

**Tech Stack:** Rust (Tauri + tokio + tokio-tungstenite), TypeScript (React + Zustand)

---

## 文件结构

### 后端 (Rust)
- `src-tauri/src/gateway/session_manager.rs` - 新增：会话连接管理
- `src-tauri/src/gateway/handlers/streaming.rs` - 新增：流式推送逻辑
- `src-tauri/src/service/opencode_service.rs` - 修改：添加流式读取方法
- `src-tauri/src/gateway/router.rs` - 修改：集成 SessionManager
- `src-tauri/src/gateway/server.rs` - 修改：支持按会话路由
- `src-tauri/src/gateway/handlers/agent.rs` - 修改：agent.run 改为异步推送

### 前端 (TypeScript)
- `src/stores/sessionWsStore.ts` - 新增：每会话 WebSocket 管理
- `src/stores/chatStore.ts` - 修改：添加流式事件处理
- `src/components/workspace/ChatPanel.tsx` - 修改：isTyping 展示逻辑
- `src/pages/WorkspacePage.tsx` - 修改：切换会话时重建连接

---

## Task 1: 后端 SessionManager

**Files:**
- Create: `src-tauri/src/gateway/session_manager.rs`
- Modify: `src-tauri/src/gateway/mod.rs`

- [ ] **Step 1: 创建 SessionManager**

```rust
// src-tauri/src/gateway/session_manager.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub struct ConnectionHandle {
    pub id: String,
    pub sender: tokio::sync::mpsc::UnboundedSender<Message>,
}

pub struct SessionManager {
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, session_id: String, handle: ConnectionHandle) {
        let mut conns = self.connections.write().await;
        conns.insert(session_id, handle);
    }

    pub async fn unregister(&self, session_id: &str) {
        let mut conns = self.connections.write().await;
        conns.remove(session_id);
    }

    pub async fn send_to_session(&self, session_id: &str, msg: Message) -> Result<(), String> {
        let conns = self.connections.read().await;
        if let Some(handle) = conns.get(session_id) {
            handle.sender.send(msg)
                .map_err(|e| format!("Failed to send: {}", e))?;
            Ok(())
        } else {
            Err(format!("Session {} not found", session_id))
        }
    }
}
```

- [ ] **Step 2: 注册到 gateway mod**

```rust
// src-tauri/src/gateway/mod.rs
pub mod session_manager;
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/gateway/session_manager.rs src-tauri/src/gateway/mod.rs
git commit -m "feat: add SessionManager for per-session WebSocket"
```

---

## Task 2: 后端流式推送

**Files:**
- Create: `src-tauri/src/gateway/handlers/streaming.rs`
- Modify: `src-tauri/src/gateway/handlers/mod.rs`

- [ ] **Step 1: 创建流式推送模块**

```rust
// src-tauri/src/gateway/handlers/streaming.rs
use crate::gateway::session_manager::SessionManager;
use crate::service::opencode_service::OpencodeService;
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::process::Stdio;
use tokio_tungstenite::tungstenite::Message;

pub async fn stream_opencode_output(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    conversation_id: String,
    message: String,
    opencode_session_id: Option<String>,
) {
    let attach_url = opencode_service.get_attach_url();
    
    let mut cmd = Command::new("opencode");
    cmd.arg("run")
        .arg(&message)
        .arg("--format").arg("json")
        .arg("--attach").arg(&attach_url);

    if let Some(sid) = &opencode_session_id {
        cmd.arg("--session").arg(sid);
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = send_error(&session_manager, &conversation_id, &format!("Failed to spawn: {}", e)).await;
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = send_error(&session_manager, &conversation_id, "Failed to capture stdout").await;
            return;
        }
    };

    let mut reader = BufReader::new(stdout).lines();
    let mut session_id_result = String::new();

    // 发送 thinking 事件
    let _ = send_event(
        &session_manager,
        &conversation_id,
        "agent.thinking",
        json!({ "content": "AI 正在思考..." })
    ).await;

    // 逐行读取并推送
    while let Ok(Some(line)) = reader.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
            // 提取 session ID
            if session_id_result.is_empty() {
                if let Some(sid) = event.get("sessionID").and_then(|v| v.as_str()) {
                    session_id_result = sid.to_string();
                }
            }

            // 解析事件类型
            let event_type = event.get("type").and_then(|v| v.as_str());
            let method = match event_type {
                Some("step_start") => "agent.thinking",
                Some("text") => "agent.token",
                Some("step_finish") => "agent.done",
                _ => continue,
            };

            let _ = send_event(
                &session_manager,
                &conversation_id,
                method,
                event.get("part").cloned().unwrap_or(event)
            ).await;
        }
    }

    // 发送 done 事件
    let _ = send_event(
        &session_manager,
        &conversation_id,
        "agent.done",
        json!({ "sessionID": session_id_result })
    ).await;

    // 等待进程结束
    let _ = child.wait().await;
}

async fn send_event(
    session_manager: &SessionManager,
    conversation_id: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<(), String> {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });
    
    session_manager.send_to_session(
        conversation_id,
        Message::Text(notification.to_string())
    ).await
}

async fn send_error(
    session_manager: &SessionManager,
    conversation_id: &str,
    message: &str,
) -> Result<(), String> {
    send_event(
        session_manager,
        conversation_id,
        "agent.error",
        json!({ "message": message })
    ).await
}
```

- [ ] **Step 2: 注册 handlers mod**

```rust
// src-tauri/src/gateway/handlers/mod.rs
pub mod streaming;
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/gateway/handlers/streaming.rs src-tauri/src/gateway/handlers/mod.rs
git commit -m "feat: add streaming push for opencode output"
```

---

## Task 3: 修改后端 Router 和 Server

**Files:**
- Modify: `src-tauri/src/gateway/router.rs`
- Modify: `src-tauri/src/gateway/server.rs`
- Modify: `src-tauri/src/gateway/connection.rs`

- [ ] **Step 1: 修改 Router 集成 SessionManager**

```rust
// src-tauri/src/gateway/router.rs
use super::session_manager::SessionManager;
// ... 其他 imports

pub struct GatewayRouter {
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
    agent_service: Arc<AgentService>,
    db_service: Arc<DbService>,
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
}

impl GatewayRouter {
    pub fn new(
        agent_service: Arc<AgentService>,
        db_service: Arc<DbService>,
        opencode_service: Arc<OpencodeService>,
        session_manager: Arc<SessionManager>,
    ) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            agent_service,
            db_service,
            opencode_service,
            session_manager,
        }
    }
    
    pub fn session_manager(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }
    
    // ... 其他方法
}
```

- [ ] **Step 2: 修改 Server 支持按会话路由**

```rust
// src-tauri/src/gateway/server.rs
// 修改 handle_connection 接收 conversation_id
pub async fn handle_connection(
    conn_id: String,
    conversation_id: String,  // 新增
    ws_stream: WebSocketStream<TcpStream>,
    router: Arc<GatewayRouter>,
) {
    // ... 现有代码 ...
    
    // 注册到 SessionManager
    let session_mgr = router.session_manager();
    session_mgr.register(conversation_id.clone(), handle.clone()).await;
    
    // ... 处理消息 ...
    
    // 注销
    session_mgr.unregister(&conversation_id).await;
    router.unregister_connection(&conn_id).await;
}
```

- [ ] **Step 3: 修改 agent.run handler**

```rust
// src-tauri/src/gateway/handlers/agent.rs
use crate::gateway::handlers::streaming::stream_opencode_output;
use crate::gateway::session_manager::SessionManager;

async fn handle_run(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    conversation_id: String,  // 从 URL 提取
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let message = req.params.get("message").and_then(|v| v.as_str());
    let session_id = req.params.get("sessionId").and_then(|v| v.as_str());
    
    if message.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required param: message", None);
    }
    
    // 启动异步流式任务
    let opencode_service_clone = opencode_service.clone();
    let session_manager_clone = session_manager.clone();
    let message = message.unwrap().to_string();
    let session_id = session_id.map(|s| s.to_string());
    
    tokio::spawn(async move {
        stream_opencode_output(
            opencode_service_clone,
            session_manager_clone,
            conversation_id,
            message,
            session_id,
        ).await;
    });
    
    // 立即返回 started
    JsonRpcResponse::success(req.id, json!({"status": "started"}))
}
```

- [ ] **Step 4: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/gateway/router.rs src-tauri/src/gateway/server.rs src-tauri/src/gateway/connection.rs src-tauri/src/gateway/handlers/agent.rs
git commit -m "feat: integrate SessionManager and streaming into router"
```

---

## Task 4: 前端 Session WebSocket Store

**Files:**
- Create: `src/stores/sessionWsStore.ts`

- [ ] **Step 1: 创建每会话 WebSocket 管理**

```typescript
// src/stores/sessionWsStore.ts
import { create } from 'zustand';

interface SessionWsState {
  connections: Map<string, WebSocket>;
  
  connect: (conversationId: string) => WebSocket;
  disconnect: (conversationId: string) => void;
  disconnectAll: () => void;
  send: (conversationId: string, message: unknown) => void;
}

const WS_BASE_URL = 'ws://127.0.0.1:9527/ws';

export const useSessionWsStore = create<SessionWsState>((set, get) => ({
  connections: new Map(),

  connect: (conversationId: string) => {
    const { connections } = get();
    
    // 如果已连接，先断开
    if (connections.has(conversationId)) {
      const oldWs = connections.get(conversationId);
      oldWs?.close();
    }
    
    const ws = new WebSocket(`${WS_BASE_URL}/${conversationId}`);
    
    ws.onopen = () => {
      console.log(`[SessionWS] Connected to ${conversationId}`);
    };
    
    ws.onclose = () => {
      console.log(`[SessionWS] Disconnected from ${conversationId}`);
      set((state) => {
        const newConnections = new Map(state.connections);
        newConnections.delete(conversationId);
        return { connections: newConnections };
      });
    };
    
    ws.onerror = (error) => {
      console.error(`[SessionWS] Error for ${conversationId}:`, error);
    };
    
    set((state) => {
      const newConnections = new Map(state.connections);
      newConnections.set(conversationId, ws);
      return { connections: newConnections };
    });
    
    return ws;
  },

  disconnect: (conversationId: string) => {
    const { connections } = get();
    const ws = connections.get(conversationId);
    if (ws) {
      ws.close();
    }
  },

  disconnectAll: () => {
    const { connections } = get();
    connections.forEach((ws) => ws.close());
    set({ connections: new Map() });
  },

  send: (conversationId: string, message: unknown) => {
    const { connections } = get();
    const ws = connections.get(conversationId);
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(message));
    }
  },
}));
```

- [ ] **Step 2: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.app.json`
Expected: PASS (或只有已有错误)

- [ ] **Step 3: Commit**

```bash
git add src/stores/sessionWsStore.ts
git commit -m "feat: add per-session WebSocket store"
```

---

## Task 5: 修改前端 ChatStore 支持流式

**Files:**
- Modify: `src/stores/chatStore.ts`

- [ ] **Step 1: 添加流式状态和方法**

```typescript
// src/stores/chatStore.ts
import { useSessionWsStore } from './sessionWsStore';

interface ChatState {
  // ... 现有状态 ...
  isTyping: boolean;
  
  // ... 现有方法 ...
  appendToken: (token: string) => void;
  setIsTyping: (isTyping: boolean) => void;
  handleStreamEvent: (event: { method: string; params: unknown }) => void;
}

export const useChatStore = create<ChatState>((set, get) => ({
  // ... 现有状态 ...
  isTyping: false,

  sendMessage: async (content: string) => {
    const { currentConversationId, opencodeSessionId, messages } = get();
    
    if (!currentConversationId) {
      throw new Error('No active conversation');
    }

    // 创建 AI 消息占位符
    const assistantMsgId = `msg-${Date.now()}`;
    set((state) => ({
      messages: [
        ...state.messages,
        {
          id: assistantMsgId,
          conversation_id: currentConversationId,
          role: 'assistant' as const,
          content: '',
          steps: [],
          is_complete: false,
          created_at: new Date().toISOString(),
        },
      ],
      isTyping: true,
      isStreaming: true,
    }));

    // 发送请求到对应会话的 WebSocket
    const sessionWs = useSessionWsStore.getState();
    sessionWs.send(currentConversationId, {
      jsonrpc: '2.0',
      id: `req-${Date.now()}`,
      method: 'agent.run',
      params: {
        message: content,
        sessionId: opencodeSessionId || undefined,
      },
    });
  },

  appendToken: (token: string) => {
    set((state) => {
      const msgs = [...state.messages];
      const lastMsg = msgs[msgs.length - 1];
      
      if (lastMsg && lastMsg.role === 'assistant' && !lastMsg.is_complete) {
        lastMsg.content = (lastMsg.content || '') + token;
      }
      
      return { messages: msgs };
    });
  },

  setIsTyping: (isTyping: boolean) => {
    set({ isTyping });
  },

  handleStreamEvent: (event: { method: string; params: unknown }) => {
    const { appendToken, setIsTyping, setIsStreaming } = get();
    
    switch (event.method) {
      case 'agent.thinking':
        // 可以记录思考内容
        break;
        
      case 'agent.token': {
        const params = event.params as { text?: string };
        if (params.text) {
          // 第一个 token 到达，关闭 isTyping
          setIsTyping(false);
          appendToken(params.text);
        }
        break;
      }
        
      case 'agent.done': {
        const params = event.params as { sessionID?: string };
        if (params.sessionID) {
          set({ opencodeSessionId: params.sessionID });
        }
        setIsStreaming(false);
        
        // 标记最后一条消息完成
        set((state) => {
          const msgs = [...state.messages];
          const lastMsg = msgs[msgs.length - 1];
          if (lastMsg && lastMsg.role === 'assistant') {
            lastMsg.is_complete = true;
          }
          return { messages: msgs };
        });
        break;
      }
        
      case 'agent.error': {
        const params = event.params as { message?: string };
        console.error('Agent error:', params.message);
        setIsStreaming(false);
        setIsTyping(false);
        break;
      }
    }
  },
  
  // ... 其他现有方法 ...
}));
```

- [ ] **Step 2: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.app.json`
Expected: PASS (或只有已有错误)

- [ ] **Step 3: Commit**

```bash
git add src/stores/chatStore.ts
git commit -m "feat: add streaming support to ChatStore"
```

---

## Task 6: 修改前端 ChatPanel 展示 isTyping

**Files:**
- Modify: `src/components/workspace/ChatPanel.tsx`

- [ ] **Step 1: 添加 isTyping 展示**

```tsx
// src/components/workspace/ChatPanel.tsx
// 在消息列表末尾添加 isTyping 展示

const ChatPanel = () => {
  const messages = useChatStore((s) => s.messages);
  const isTyping = useChatStore((s) => s.isTyping);
  const isStreaming = useChatStore((s) => s.isStreaming);
  
  // ... 现有代码 ...
  
  return (
    <div className="...">
      {/* 消息列表 */}
      {messages.map((msg) => (
        <MessageItem key={msg.id} message={msg} />
      ))}
      
      {/* isTyping 展示 */}
      {isTyping && (
        <div className="flex gap-3 px-4 py-3">
          <div className="w-7 h-7 rounded shrink-0 flex items-center justify-center bg-accent">
            <Bot className="w-3.5 h-3.5 text-primary" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-[11px] text-muted-foreground mb-1">AI助手</div>
            <div className="flex items-center gap-1">
              <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
              <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse [animation-delay:0.2s]" />
              <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse [animation-delay:0.4s]" />
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
```

- [ ] **Step 2: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.app.json`
Expected: PASS (或只有已有错误)

- [ ] **Step 3: Commit**

```bash
git add src/components/workspace/ChatPanel.tsx
git commit -m "feat: add isTyping indicator in ChatPanel"
```

---

## Task 7: 修改 WorkspacePage 管理会话连接

**Files:**
- Modify: `src/pages/WorkspacePage.tsx`

- [ ] **Step 1: 切换会话时重建 WebSocket**

```typescript
// src/pages/WorkspacePage.tsx
import { useSessionWsStore } from '@/stores/sessionWsStore';
import { useChatStore } from '@/stores/chatStore';

const WorkspacePage = () => {
  const activeConversation = ...; // 当前激活的会话
  const sessionWs = useSessionWsStore();
  const handleStreamEvent = useChatStore((s) => s.handleStreamEvent);
  
  // 激活会话时建立 WebSocket
  useEffect(() => {
    if (!activeConversation) return;
    
    const ws = sessionWs.connect(activeConversation.id);
    
    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        
        // 处理流式事件
        if (data.method && !data.id) {
          handleStreamEvent(data);
        }
      } catch (e) {
        console.error('Failed to parse WebSocket message:', e);
      }
    };
    
    return () => {
      sessionWs.disconnect(activeConversation.id);
    };
  }, [activeConversation?.id]);
  
  // ... 其他代码 ...
};
```

- [ ] **Step 2: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.app.json`
Expected: PASS (或只有已有错误)

- [ ] **Step 3: Commit**

```bash
git add src/pages/WorkspacePage.tsx
git commit -m "feat: manage per-session WebSocket connections"
```

---

## Task 8: 集成测试

**Files:**
- All modified files

- [ ] **Step 1: 构建后端**

Run: `pnpm tauri build`
Expected: 编译成功

- [ ] **Step 2: 启动服务**

```bash
bash run-desktop.sh
```

- [ ] **Step 3: 测试单会话流式**

1. 打开浏览器访问 `http://localhost:5173`
2. 登录并选择 Agent
3. 进入工作区
4. 发送消息"你好"
5. 验证：
   - 显示用户消息
   - 显示 isTyping（三个点）
   - 第一个 token 到达后 isTyping 消失
   - 文字逐字显示
   - 最后显示"已完成"

- [ ] **Step 4: 测试切换会话**

1. 创建新会话
2. 切换回旧会话
3. 验证 WebSocket 正确重建
4. 发送消息验证流式正常

- [ ] **Step 5: 测试多会话**

1. 在会话 A 发送消息
2. 快速切换到会话 B 发送消息
3. 验证两个会话的流式输出互不干扰

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: complete per-session WebSocket streaming implementation"
```

---

## 回滚方案

如果实现过程中出现问题，可以回滚到当前 WebSocket 版本：

```bash
git checkout master
pnpm tauri build
bash run-desktop.sh
```

---

## 成功标准

- [ ] 每会话独立 WebSocket
- [ ] 激活会话建立连接，离开关闭
- [ ] AI 回复逐字显示（流式）
- [ ] isTyping 在第一个 token 到达后消失
- [ ] 只有一个 AI 消息（无重复）
- [ ] 切换会话时正确重建连接
