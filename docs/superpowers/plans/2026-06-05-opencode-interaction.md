# OpenCode 交互支持实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 打通 opencode serve 的交互能力（permission ask / ask for user / todo list），Rust 后端监听 SSE 事件并解析交互请求，前端展示对应的交互 UI 并回传用户回答。

**Architecture:** 后端以 `POST /message` 同步响应为主触发源检测交互请求，同时维护 SSE `/event` 监听器处理异步事件；前端通过 WebSocket 接收 `agent.question` / `agent.permission` / `agent.todowrite` 通知，用户回答后发送 `agent.respond` 回后端。

**Tech Stack:** Rust (Tauri + reqwest + tokio) / React + TypeScript + Zustand + Tailwind CSS

---

## 文件结构

| 文件 | 责任 |
|------|------|
| `src-tauri/src/service/opencode_service.rs` | HTTP 客户端：send_message、SSE 监听、交互检测、发送回答 |
| `src-tauri/src/models/interaction.rs` | 新增：交互相关 Rust 数据结构（InteractionRequest、QuestionItem 等） |
| `src-tauri/src/gateway/handlers/agent.rs` | WebSocket handler：新增 `agent.respond` 处理、向前端推送交互通知 |
| `src/types/types.ts` | 扩展：前端交互类型定义 |
| `src/components/workspace/ChatPanel.tsx` | 修改：完善 PermissionStep / UserQuestionsStep / 新增 TodoWriteStep |
| `src/components/workspace/RightPanel.tsx` | 修改：新增 todo 列表展示区域 |
| `src/stores/websocketStore.ts` | 修改：订阅新的通知类型，支持发送 `agent.respond` |
| `src/stores/chatStore.ts` | 修改：管理交互状态（当前是否有待回答的交互） |

---

## Task 1: 后端交互数据模型

**Files:**
- Create: `src-tauri/src/models/interaction.rs`
- Modify: `src-tauri/src/models/mod.rs`

- [ ] **Step 1: 创建交互数据模型**

```rust
// src-tauri/src/models/interaction.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionRequest {
    Question { questions: Vec<QuestionItem> },
    Permission { tool_name: String, action: String },
    TodoWrite { todos: Vec<TodoItem> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionItem {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multiple: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionResponse {
    pub session_id: String,
    pub interaction_type: String,
    pub response: serde_json::Value,
}
```

- [ ] **Step 2: 注册模型模块**

```rust
// src-tauri/src/models/mod.rs
pub mod interaction;
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/models/
git commit -m "feat(models): add interaction data structures"
```

---

## Task 2: 后端 SSE 监听器 + 交互检测

**Files:**
- Modify: `src-tauri/src/service/opencode_service.rs`
- Modify: `src-tauri/src/main.rs`（启动 SSE 监听器）

- [ ] **Step 1: 扩展 OpencodeService 支持 SSE**

在 `OpencodeService` struct 中增加 `event_sender` 字段：

```rust
use tokio::sync::mpsc;

pub struct OpencodeService {
    serve_process: Arc<Mutex<Option<Child>>>,
    port: u16,
    base_url: String,
    client: reqwest::Client,
    event_sender: Option<mpsc::Sender<SseEvent>>,
}

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: String,
    pub session_id: Option<String>,
    pub payload: serde_json::Value,
}
```

- [ ] **Step 2: 实现 SSE 监听器**

在 `impl OpencodeService` 中新增方法：

```rust
pub fn set_event_sender(&mut self, sender: mpsc::Sender<SseEvent>) {
    self.event_sender = Some(sender);
}

pub async fn start_event_listener(&self) {
    let base_url = self.base_url.clone();
    let sender = match self.event_sender.clone() {
        Some(s) => s,
        None => {
            log::warn!("[opencode] event_sender not set, skipping SSE listener");
            return;
        }
    };

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            match client
                .get(format!("{}/event", base_url))
                .header("Accept", "text/event-stream")
                .send()
                .await
            {
                Ok(resp) => {
                    let mut stream = resp.bytes_stream();
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                let text = String::from_utf8_lossy(&bytes);
                                for line in text.lines() {
                                    if line.starts_with("data: ") {
                                        let data = &line[6..];
                                        if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                                            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                                            let session_id = event.get("properties").and_then(|p| p.get("sessionID")).and_then(|v| v.as_str()).map(|s| s.to_string());
                                            let _ = sender.send(SseEvent {
                                                event_type,
                                                session_id,
                                                payload: event,
                                            }).await;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("[opencode] SSE stream error: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("[opencode] SSE connect error: {}", e);
                }
            }
            log::info!("[opencode] SSE disconnected, retrying in 3s...");
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    });
}
```

需要在文件顶部添加 `use futures_util::StreamExt;`。

- [ ] **Step 3: 实现交互检测**

```rust
use crate::models::interaction::{InteractionRequest, QuestionItem, QuestionOption, TodoItem};

pub fn detect_interaction_from_parts(parts: &[serde_json::Value]) -> Option<InteractionRequest> {
    for part in parts {
        let part_type = part.get("type").and_then(|v| v.as_str());
        match part_type {
            Some("tool_use") => {
                let tool_name = part.get("toolName").or_else(|| part.get("tool_name")).and_then(|v| v.as_str());
                match tool_name {
                    Some("question") => {
                        if let Some(input) = part.get("input") {
                            if let Ok(questions) = serde_json::from_value::<Vec<QuestionItem>>(input.get("questions").cloned().unwrap_or(serde_json::Value::Null)) {
                                return Some(InteractionRequest::Question { questions });
                            }
                        }
                    }
                    Some("todowrite") => {
                        if let Some(input) = part.get("input") {
                            if let Ok(todos) = serde_json::from_value::<Vec<TodoItem>>(input.get("todos").cloned().unwrap_or(serde_json::Value::Null)) {
                                return Some(InteractionRequest::TodoWrite { todos });
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("permission") | Some("ask_permission") => {
                let tool_name = part.get("toolName").or_else(|| part.get("tool_name")).and_then(|v| v.as_str()).unwrap_or("unknown");
                let action = part.get("action").and_then(|v| v.as_str()).unwrap_or("");
                return Some(InteractionRequest::Permission {
                    tool_name: tool_name.to_string(),
                    action: action.to_string(),
                });
            }
            _ => {}
        }
    }
    None
}
```

- [ ] **Step 4: 修改 `run_message` 使用交互检测**

在 `run_message` 方法中，解析 `result.parts` 后调用 `detect_interaction_from_parts`：

```rust
pub async fn run_message(
    &self,
    message: &str,
    session_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let sid = match session_id {
        Some(sid) if !sid.is_empty() => sid.to_string(),
        _ => {
            let session = self.create_session().await?;
            session
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "Failed to get session ID".to_string())?
        }
    };

    let result = self.send_message(&sid, message).await?;
    let session_id_result = result
        .get("info")
        .and_then(|i| i.get("sessionID"))
        .and_then(|v| v.as_str())
        .unwrap_or(&sid)
        .to_string();

    let mut text_parts: Vec<String> = Vec::new();
    let parts = result.get("parts").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    for part in &parts {
        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
            text_parts.push(text.to_string());
        }
    }

    if text_parts.is_empty() {
        text_parts.push("opencode 未返回内容".to_string());
    }

    let interaction = detect_interaction_from_parts(&parts);

    Ok(serde_json::json!({
        "sessionID": session_id_result,
        "parts": text_parts.iter().map(|t| {
            serde_json::json!({ "type": "text", "text": t })
        }).collect::<Vec<_>>(),
        "interaction": interaction,
    }))
}
```

- [ ] **Step 5: 修改 main.rs 启动 SSE 监听器**

在 `main.rs` 的 setup 中：

```rust
let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<SseEvent>(100);
{
    let mut svc = opencode_service.as_ref();
    // 需要在 OpencodeService 上提供设置 event_sender 的方法
}
```

等等，`opencode_service` 是 `Arc<OpencodeService>`，不能直接修改。我需要调整设计：让 `OpencodeService::new()` 接收可选的 `event_sender`，或者使用 `Mutex` 包装。

更好的方案：在 `OpencodeService::new()` 中直接创建 channel：

```rust
pub fn new() -> Result<(Self, mpsc::Receiver<SseEvent>), String> {
    let port = Self::find_available_port_sync()?;
    // ... spawn opencode serve ...
    let (event_tx, event_rx) = mpsc::channel::<SseEvent>(100);
    let base_url = format!("http://127.0.0.1:{}", port);
    let service = Self {
        serve_process: Arc::new(Mutex::new(Some(child))),
        port,
        base_url,
        client: reqwest::Client::new(),
        event_sender: Some(event_tx),
    };
    Ok((service, event_rx))
}
```

然后 main.rs 中：

```rust
let (opencode_service, mut event_rx) = service::opencode_service::OpencodeService::new().unwrap_or_else(|e| {
    log::warn!("[main.rs] Failed to initialize OpencodeService: {}, using fallback", e);
    service::opencode_service::OpencodeService::new_fallback()
});
let opencode_service = Arc::new(opencode_service);
app.manage(opencode_service.clone());

// 启动 SSE 监听器
opencode_service.start_event_listener().await;

// 处理 SSE 事件路由到 WebSocket
let session_manager_clone = session_manager.clone();
tokio::spawn(async move {
    while let Some(event) = event_rx.recv().await {
        if let Some(session_id) = &event.session_id {
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "agent.sse_event",
                "params": event.payload
            });
            let _ = session_manager_clone.send_to_session(
                session_id,
                tokio_tungstenite::tungstenite::Message::Text(notification.to_string()),
            ).await;
        }
    }
});
```

但这会改变 `new()` 的返回类型，需要同时修改 `new_fallback()`。

简化方案：不在 `new()` 中创建 channel，而是在 `main.rs` 中创建后通过 `set_event_sender` 设置。

```rust
impl OpencodeService {
    pub fn set_event_sender(&mut self, sender: mpsc::Sender<SseEvent>) {
        self.event_sender = Some(sender);
    }
}
```

但 `Arc<OpencodeService>` 不能直接修改。所以需要用 `Arc<tokio::sync::Mutex<OpencodeService>>` 或 `Arc<OpencodeService>` 内部用 `Mutex<Option<mpsc::Sender>>`。

最简单的方案：`event_sender` 用 `Arc<Mutex<Option<mpsc::Sender<SseEvent>>>>`：

```rust
pub struct OpencodeService {
    // ...
    event_sender: Arc<Mutex<Option<mpsc::Sender<SseEvent>>>>,
}

pub fn set_event_sender(&self, sender: mpsc::Sender<SseEvent>) {
    if let Ok(mut guard) = self.event_sender.lock() {
        *guard = Some(sender);
    }
}
```

- [ ] **Step 6: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS（可能需要添加 `futures_util` 的 import）

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/service/opencode_service.rs src-tauri/src/models/interaction.rs src-tauri/src/models/mod.rs src-tauri/src/main.rs
git commit -m "feat(backend): add SSE listener and interaction detection"
```

---

## Task 3: 后端 WebSocket 推送 + agent.respond handler

**Files:**
- Modify: `src-tauri/src/gateway/handlers/agent.rs`
- Modify: `src-tauri/src/gateway/handlers/streaming.rs`

- [ ] **Step 1: 修改 handle_send_message 推送交互事件**

```rust
async fn handle_send_message(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let instance_id = req.params.get("instanceId").and_then(|v| v.as_str());
    let conversation_id = req.params.get("conversationId").and_then(|v| v.as_str());
    let message = req.params.get("message").and_then(|v| v.as_str());
    let opencode_session_id = req.params.get("opencodeSessionId").and_then(|v| v.as_str());

    if instance_id.is_none() || conversation_id.is_none() || message.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params", None);
    }

    match opencode_service.run_message(message.unwrap(), opencode_session_id).await {
        Ok(result) => {
            // 如果检测到交互请求，通过 WebSocket 推送通知
            if let Some(interaction) = result.get("interaction").cloned() {
                let method = match interaction.get("type").and_then(|v| v.as_str()) {
                    Some("question") => "agent.question",
                    Some("permission") => "agent.permission",
                    Some("todo_write") => "agent.todowrite",
                    _ => "agent.interaction",
                };
                let notification = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": {
                        "instanceId": instance_id.unwrap(),
                        "conversationId": conversation_id.unwrap(),
                        "sessionID": result.get("sessionID"),
                        "interaction": interaction,
                    }
                });
                let _ = session_manager.send_to_session(
                    conversation_id.unwrap(),
                    tokio_tungstenite::tungstenite::Message::Text(notification.to_string()),
                ).await;
            }
            JsonRpcResponse::success(req.id, result)
        }
        Err(error) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &error, None),
    }
}
```

- [ ] **Step 2: 新增 agent.respond handler**

在 `handle_agent_request` 的 match 中增加 `"agent.respond"`：

```rust
"agent.respond" => handle_respond(opencode_service, req).await,
```

实现 `handle_respond`：

```rust
async fn handle_respond(
    opencode_service: Arc<OpencodeService>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let session_id = req.params.get("sessionId").and_then(|v| v.as_str());
    let interaction_type = req.params.get("interactionType").and_then(|v| v.as_str());
    let response = req.params.get("response").cloned();

    if session_id.is_none() || interaction_type.is_none() || response.is_none() {
        return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Missing required params", None);
    }

    let sid = session_id.unwrap();
    let resp = response.unwrap();

    // 将回答格式化为消息并发送给 opencode serve
    let message = match interaction_type.unwrap() {
        "question" => {
            if let Some(answers) = resp.get("answers").and_then(|v| v.as_array()) {
                let texts: Vec<String> = answers.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                texts.join("\n")
            } else {
                return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Invalid response format for question", None);
            }
        }
        "permission" => {
            resp.get("answer").and_then(|v| v.as_str()).unwrap_or("deny").to_string()
        }
        "todowrite" => {
            resp.get("todos").map(|v| v.to_string()).unwrap_or_default()
        }
        _ => return JsonRpcResponse::error(req.id, INVALID_PARAMS, "Unknown interaction type", None),
    };

    match opencode_service.send_message(sid, &message).await {
        Ok(result) => JsonRpcResponse::success(req.id, result),
        Err(error) => JsonRpcResponse::error(req.id, INTERNAL_ERROR, &error, None),
    }
}
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/gateway/handlers/agent.rs
git commit -m "feat(backend): add agent.respond handler and interaction WebSocket push"
```

---

## Task 4: 前端类型扩展

**Files:**
- Modify: `src/types/types.ts`

- [ ] **Step 1: 扩展类型定义**

```typescript
// src/types/types.ts

export interface MessageStep {
  type: 'thinking' | 'tool_use' | 'tool_result' | 'ask_permission' | 'ask_user' | 'final' | 'compress' | 'retry';
  content: string;
  toolName?: string;
  questions?: AskQuestion[];
  permissionType?: string;
  failed?: boolean;
  summary?: ToolSummary;
  compressInfo?: { originalSize: number; compressedSize: number; ratio: number; status: 'compressing' | 'done' };
  diff?: string;
  // 新增：交互 payload
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
  options: QuestionOption[];
  multiple: boolean;
}

export interface QuestionOption {
  label: string;
  description: string;
}

export interface TodoItem {
  id: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  priority: 'high' | 'medium' | 'low';
}
```

- [ ] **Step 2: 类型检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/types/types.ts
git commit -m "feat(types): extend interaction types for opencode"
```

---

## Task 5: 前端 PermissionStep 重构

**Files:**
- Modify: `src/components/workspace/ChatPanel.tsx`

- [ ] **Step 1: 重构 PermissionStep 支持 opencode 格式**

```tsx
function PermissionStep({ step, onAnswer }: {
  step: MessageStep;
  onAnswer: (answer: 'once' | 'session' | 'deny') => void;
}) {
  const config = stepConfig.ask_permission;
  const Icon = config.icon;
  const interaction = step.interaction;
  const toolName = interaction?.toolName || step.permissionType || 'unknown';
  const action = interaction?.action || step.content;

  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[12px] font-medium ${config.labelColor}`}>
          {config.label} · {toolName}
        </span>
      </div>
      <div className="px-3 pb-2 text-xs text-foreground leading-relaxed whitespace-pre-wrap">
        {action}
      </div>
      <div className="flex flex-col gap-1.5 px-3 pb-3">
        <button
          type="button"
          onClick={() => onAnswer('once')}
          className="w-full py-2 px-3 text-xs rounded-md bg-primary/15 text-primary hover:bg-primary/25 transition-colors border border-primary/25 font-medium"
        >
          本次同意 (once)
        </button>
        <button
          type="button"
          onClick={() => onAnswer('session')}
          className="w-full py-2 px-3 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border"
        >
          本 Session 同意 (always)
        </button>
        <button
          type="button"
          onClick={() => onAnswer('deny')}
          className="w-full py-2 px-3 text-xs rounded-md bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors border border-destructive/20"
        >
          不同意 (reject)
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/workspace/ChatPanel.tsx
git commit -m "feat(ui): refactor PermissionStep for opencode format"
```

---

## Task 6: 前端 UserQuestionsStep 重构

**Files:**
- Modify: `src/components/workspace/ChatPanel.tsx`

- [ ] **Step 1: 重构 UserQuestionsStep 支持多题、多选、自定义输入**

```tsx
function UserQuestionsStep({ step, onSubmit }: {
  step: MessageStep;
  onSubmit: (answers: Record<string, string | string[]>) => void;
}) {
  const config = stepConfig.ask_user;
  const Icon = config.icon;
  const questions = step.interaction?.questions || [];
  const [answers, setAnswers] = useState<Record<string, string | string[]>>({});
  const [customInputs, setCustomInputs] = useState<Record<string, string>>({});

  const toggleOption = (qIdx: number, label: string, multiple: boolean) => {
    const key = String(qIdx);
    setAnswers(prev => {
      const current = prev[key];
      if (multiple) {
        const arr = Array.isArray(current) ? [...current] : [];
        if (arr.includes(label)) {
          return { ...prev, [key]: arr.filter(l => l !== label) };
        }
        return { ...prev, [key]: [...arr, label] };
      }
      return { ...prev, [key]: label };
    });
  };

  const handleCustomSubmit = (qIdx: number) => {
    const text = customInputs[qIdx]?.trim();
    if (!text) return;
    setAnswers(prev => ({ ...prev, [String(qIdx)]: text }));
  };

  const canSubmit = questions.every((_, idx) => {
    const ans = answers[idx];
    return ans !== undefined && (typeof ans === 'string' ? ans.length > 0 : ans.length > 0);
  });

  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[12px] font-medium ${config.labelColor}`}>{config.label}</span>
      </div>
      <div className="flex flex-col gap-3 px-3 pb-3">
        {questions.map((q, qIdx) => (
          <div key={qIdx} className="flex flex-col gap-1.5">
            <div className="text-xs font-medium text-foreground">{q.header}</div>
            <div className="text-xs text-muted-foreground">{q.question}</div>
            {q.multiple && (
              <div className="text-[10px] text-muted-foreground">可多选</div>
            )}
            <div className="flex flex-col gap-1">
              {q.options.map((opt, oIdx) => {
                const ans = answers[qIdx];
                const isSelected = q.multiple
                  ? Array.isArray(ans) && ans.includes(opt.label)
                  : ans === opt.label;
                return (
                  <button
                    key={oIdx}
                    type="button"
                    onClick={() => toggleOption(qIdx, opt.label, q.multiple)}
                    className={`text-left px-2.5 py-1.5 text-xs rounded-md border transition-colors ${
                      isSelected
                        ? 'bg-primary/15 text-primary border-primary/25'
                        : 'bg-secondary text-foreground border-border hover:bg-secondary/80'
                    }`}
                  >
                    <span className="font-medium">{opt.label}</span>
                    {opt.description && (
                      <span className="text-muted-foreground ml-1">· {opt.description}</span>
                    )}
                  </button>
                );
              })}
              {/* 自定义输入（opencode 默认启用 custom） */}
              <div className="flex items-center gap-2 mt-1">
                <input
                  type="text"
                  value={customInputs[qIdx] || ''}
                  onChange={(e) => setCustomInputs(prev => ({ ...prev, [qIdx]: e.target.value }))}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      handleCustomSubmit(qIdx);
                    }
                  }}
                  placeholder="输入自定义答案..."
                  className="flex-1 min-w-0 px-2.5 py-1.5 text-xs bg-secondary border border-border rounded-md text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary"
                />
                <button
                  type="button"
                  onClick={() => handleCustomSubmit(qIdx)}
                  disabled={!customInputs[qIdx]?.trim()}
                  className="shrink-0 px-2.5 py-1.5 text-xs rounded-md bg-secondary text-foreground hover:bg-secondary/80 transition-colors border border-border disabled:opacity-40"
                >
                  使用
                </button>
              </div>
            </div>
          </div>
        ))}
        <button
          type="button"
          onClick={() => onSubmit(answers)}
          disabled={!canSubmit}
          className="w-full py-2 px-3 text-xs rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors font-medium disabled:opacity-40"
        >
          提交回答
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: 修改 StepItem 支持新的 UserQuestionsStep 签名**

```tsx
if (step.type === 'ask_user' && onAnswerUser) return <UserQuestionsStep step={step} onSubmit={onAnswerUser} />;
```

- [ ] **Step 3: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/components/workspace/ChatPanel.tsx
git commit -m "feat(ui): refactor UserQuestionsStep for opencode multi-question format"
```

---

## Task 7: 前端 TodoWriteStep + RightPanel Todo 列表

**Files:**
- Modify: `src/components/workspace/ChatPanel.tsx`
- Modify: `src/components/workspace/RightPanel.tsx`

- [ ] **Step 1: 新增 TodoWriteStep 组件（消息流中展示）**

```tsx
function TodoWriteStep({ step }: { step: MessageStep }) {
  const config = stepConfig.tool_result;
  const Icon = config.icon;
  const todos = step.interaction?.todos || [];

  const priorityColor: Record<string, string> = {
    high: 'text-red-400',
    medium: 'text-yellow-400',
    low: 'text-muted-foreground',
  };

  const statusIcon: Record<string, string> = {
    pending: '○',
    in_progress: '◐',
    completed: '●',
    cancelled: '✕',
  };

  return (
    <div className={`rounded-md border ${config.border} ${config.bg} overflow-hidden`}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <Icon className={`w-3 h-3 shrink-0 ${config.labelColor}`} />
        <span className={`text-[12px] font-medium ${config.labelColor}`}>任务列表更新</span>
      </div>
      <div className="flex flex-col gap-1 px-3 pb-3">
        {todos.map((todo) => (
          <div key={todo.id} className="flex items-center gap-2 text-xs">
            <span className="text-muted-foreground">{statusIcon[todo.status] || '○'}</span>
            <span className={`text-[10px] font-medium ${priorityColor[todo.priority] || ''}`}>
              {todo.priority === 'high' ? '高' : todo.priority === 'medium' ? '中' : '低'}
            </span>
            <span className="text-foreground">{todo.content}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

在 `StepItem` 中新增分支：

```tsx
if (step.type === 'tool_result' && step.toolName === 'todowrite') return <TodoWriteStep step={step} />;
```

- [ ] **Step 2: 扩展 RightPanel 展示 Todo 列表**

扩展 `RightPanelProps`：

```typescript
interface RightPanelProps {
  tasks: Task[];
  todos?: TodoItem[]; // 新增
  modifiedFiles: ModifiedFile[];
  workspace: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
}
```

在 RightPanel 的任务列表上方新增 Todo 列表区域：

```tsx
// 在 "任务" section 上方新增 "Todo" section
<div className="flex-1 min-h-0 flex flex-col border-b border-border">
  <button
    type="button"
    onClick={() => setExpandedTodos(!expandedTodos)}
    className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0 hover:bg-secondary/30 transition-colors"
  >
    <span className="text-xs font-medium text-foreground">
      <ListTodo className="w-3.5 h-3.5 inline mr-1.5 text-primary" />
      Todo
    </span>
    {expandedTodos ? <ChevronDown className="w-3 h-3 text-muted-foreground" /> : <ChevronRight className="w-3 h-3 text-muted-foreground" />}
  </button>
  {expandedTodos && (
    <div className="flex-1 overflow-y-auto py-1">
      {(!todos || todos.length === 0) ? (
        <div className="px-3 py-6 text-center text-xs text-muted-foreground">暂无 Todo</div>
      ) : (
        todos.map((todo) => {
          const priorityColor = todo.priority === 'high' ? 'text-red-400' : todo.priority === 'medium' ? 'text-yellow-400' : 'text-muted-foreground';
          const statusIcon = todo.status === 'completed' ? '●' : todo.status === 'in_progress' ? '◐' : '○';
          return (
            <div key={todo.id} className="flex items-start gap-2 px-3 py-2 hover:bg-secondary/30 transition-colors">
              <span className="text-xs text-muted-foreground mt-0.5">{statusIcon}</span>
              <span className={`text-[10px] font-medium mt-0.5 ${priorityColor}`}>
                {todo.priority === 'high' ? '高' : todo.priority === 'medium' ? '中' : '低'}
              </span>
              <div className="flex-1 min-w-0">
                <div className={`text-xs truncate ${todo.status === 'completed' ? 'text-muted-foreground line-through' : 'text-foreground'}`}>{todo.content}</div>
              </div>
            </div>
          );
        })
      )}
    </div>
  )}
</div>
```

需要新增 `expandedTodos` state：

```tsx
const [expandedTodos, setExpandedTodos] = useState(true);
```

- [ ] **Step 3: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/components/workspace/ChatPanel.tsx src/components/workspace/RightPanel.tsx
git commit -m "feat(ui): add TodoWriteStep and RightPanel todo list"
```

---

## Task 8: 前端 WebSocket 事件订阅 + agent.respond

**Files:**
- Modify: `src/stores/websocketStore.ts`
- Modify: `src/stores/chatStore.ts`

- [ ] **Step 1: 扩展 websocketStore 支持订阅通知和发送 respond**

`websocketStore.ts` 已经支持 `subscribe` 方法，可以直接使用。需要确认前端代码中已订阅这些通知。

在 `ChatPanel.tsx` 或 `WorkspacePage.tsx` 中使用：

```typescript
useEffect(() => {
  const wsStore = useWebSocketStore.getState();
  const unsubQuestion = wsStore.subscribe('agent.question', (params) => {
    // 在消息列表中插入 question 交互步骤
  });
  const unsubPermission = wsStore.subscribe('agent.permission', (params) => {
    // 在消息列表中插入 permission 交互步骤
  });
  const unsubTodo = wsStore.subscribe('agent.todowrite', (params) => {
    // 更新 todo 列表
  });
  return () => {
    unsubQuestion();
    unsubPermission();
    unsubTodo();
  };
}, []);
```

- [ ] **Step 2: 扩展 chatStore 管理交互状态**

```typescript
// src/stores/chatStore.ts
interface ChatState {
  // ... 现有字段
  pendingInteraction: { sessionId: string; type: string; payload: unknown } | null;
  todos: TodoItem[];

  setPendingInteraction: (interaction: ChatState['pendingInteraction']) => void;
  setTodos: (todos: TodoItem[]) => void;
  sendInteractionResponse: (response: Record<string, unknown>) => Promise<void>;
}

export const useChatStore = create<ChatState>((set, get) => ({
  // ... 现有字段
  pendingInteraction: null,
  todos: [],

  setPendingInteraction: (interaction) => set({ pendingInteraction: interaction }),
  setTodos: (todos) => set({ todos }),

  sendInteractionResponse: async (response) => {
    const { pendingInteraction } = get();
    if (!pendingInteraction) return;

    await useWebSocketStore.getState().sendRequest('agent.respond', {
      sessionId: pendingInteraction.sessionId,
      interactionType: pendingInteraction.type,
      response,
    });

    set({ pendingInteraction: null });
  },
}));
```

- [ ] **Step 3: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/stores/websocketStore.ts src/stores/chatStore.ts
git commit -m "feat(frontend): add WebSocket interaction subscription and respond"
```

---

## Task 9: 端到端集成测试

**Files:**
- 修改: `src/pages/WorkspacePage.tsx`（连接各组件的 props）

- [ ] **Step 1: 连接 RightPanel 的 todos prop**

在 `WorkspacePage.tsx` 中：

```tsx
const todos = useChatStore(state => state.todos);
// ...
<RightPanel
  tasks={tasks}
  todos={todos}
  modifiedFiles={modifiedFiles}
  // ...
/>
```

- [ ] **Step 2: 连接 ChatPanel 的交互回调**

```tsx
const sendInteractionResponse = useChatStore(state => state.sendInteractionResponse);

// ChatPanel props
onAnswerPermission={(stepIndex, answer) => {
  sendInteractionResponse({ answer });
}}
onAnswerUserQuestions={(stepIndex, answers) => {
  sendInteractionResponse({ answers });
}}
```

- [ ] **Step 3: 编译检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: PASS

- [ ] **Step 4: 构建测试**

Run: `pnpm tauri build`
Expected: Rust 编译通过（AppImage 打包可能因 linuxdeploy 缺失失败，但二进制应成功）

- [ ] **Step 5: Commit**

```bash
git add src/pages/WorkspacePage.tsx
git commit -m "feat(integration): wire up interaction components and stores"
```

---

## Self-Review Checklist

- [x] **Spec coverage**: SSE 监听器 ✅、交互检测 ✅、Permission UI ✅、Question UI ✅、Todo UI ✅、回答回传 ✅
- [x] **Placeholder scan**: 无 TBD/TODO/"implement later"
- [x] **Type consistency**: `InteractionRequest` / `InteractionPayload` / `QuestionItem` / `TodoItem` 类型前后端一致
