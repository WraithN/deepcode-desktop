use agent_core::process::event::{ProcessEvent, QuestionItem, QuestionOption, TodoItem};
use serde_json::{json, Value};

const KEY_TYPE: &str = "type";
const KEY_PROPERTIES: &str = "properties";
const KEY_DELTA: &str = "delta";
const KEY_CONTENT: &str = "content";
const KEY_TEXT: &str = "text";
const KEY_PART: &str = "part";
const KEY_SESSION_ID: &str = "sessionID";
const KEY_INPUT: &str = "input";
const KEY_OUTPUT: &str = "output";
const KEY_STATUS: &str = "status";
const KEY_STATE: &str = "state";
const KEY_QUESTIONS: &str = "questions";
const KEY_TODOS: &str = "todos";
const KEY_MESSAGE: &str = "message";
const KEY_ERROR: &str = "error";
const KEY_NAME: &str = "name";
const KEY_ARGS: &str = "args";
const KEY_RESULT: &str = "result";
const KEY_FAILED: &str = "failed";
const KEY_TOOL_NAME: &str = "toolName";
const KEY_TOOL_NAME_ALT: &str = "tool_name";
const KEY_TOOL: &str = "tool";
const KEY_ACTION: &str = "action";
const KEY_CALL_ID: &str = "callID";
const KEY_ID: &str = "id";
const KEY_HEADER: &str = "header";
const KEY_QUESTION: &str = "question";
const KEY_OPTIONS: &str = "options";
const KEY_LABEL: &str = "label";
const KEY_DESCRIPTION: &str = "description";

const EVENT_TYPE_MESSAGE_PART_DELTA: &str = "message.part.delta";
const EVENT_TYPE_THINKING: &str = "thinking";
const EVENT_TYPE_MESSAGE_PART_UPDATED: &str = "message.part.updated";
const EVENT_TYPE_TOOL_USE: &str = "tool_use";
const EVENT_TYPE_TOOL_RESULT: &str = "tool_result";
const EVENT_TYPE_SESSION_IDLE: &str = "session.idle";
const EVENT_TYPE_SESSION_ERROR: &str = "session.error";

const PART_TYPE_TOOL: &str = "tool";
const PART_TYPE_TOOL_USE: &str = "tool_use";
const PART_TYPE_PERMISSION: &str = "permission";
const PART_TYPE_ASK_PERMISSION: &str = "ask_permission";
const STEP_TYPE_STEP_START: &str = "step-start";

const DEFAULT_UNKNOWN: &str = "unknown";
const DEFAULT_EMPTY: &str = "";
const DEFAULT_QUESTION_ID: &str = "confirm";

/// Maps an OpenCode SSE JSON payload into one or more unified [`ProcessEvent`]s.
///
/// A single payload may yield multiple events (e.g. a completed opencode tool_use
/// can be translated into both `ToolUse` and `ToolResult`), so the return type is a
/// `Vec` rather than an `Option`.
pub fn map_opencode_sse(payload: &Value) -> Vec<ProcessEvent> {
    let event_type = payload
        .get(KEY_TYPE)
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_UNKNOWN);

    match event_type {
        EVENT_TYPE_MESSAGE_PART_DELTA => {
            let delta = payload
                .get(KEY_PROPERTIES)
                .and_then(|p| p.get(KEY_DELTA))
                .and_then(|v| v.as_str());
            match delta {
                Some(d) if !d.is_empty() => vec![ProcessEvent::TextDelta { text: d.into() }],
                _ => vec![],
            }
        }
        EVENT_TYPE_THINKING => {
            let content = payload
                .get(KEY_CONTENT)
                .or_else(|| payload.get(KEY_TEXT))
                .and_then(|v| v.as_str())
                .unwrap_or(DEFAULT_EMPTY);
            vec![ProcessEvent::Thinking {
                content: content.into(),
            }]
        }
        EVENT_TYPE_MESSAGE_PART_UPDATED => {
            let Some(part) = payload.get(KEY_PROPERTIES).and_then(|p| p.get(KEY_PART)) else {
                return vec![];
            };
            match part.get(KEY_TYPE).and_then(|v| v.as_str()) {
                Some(STEP_TYPE_STEP_START) => {
                    let text = part
                        .get(KEY_TEXT)
                        .and_then(|v| v.as_str())
                        .unwrap_or(DEFAULT_EMPTY);
                    vec![ProcessEvent::Thinking {
                        content: text.into(),
                    }]
                }
                Some(PART_TYPE_TOOL) | Some(PART_TYPE_TOOL_USE) => map_tool_use_part(part),
                _ => vec![],
            }
        }
        EVENT_TYPE_TOOL_USE => map_tool_use(payload),
        EVENT_TYPE_TOOL_RESULT => map_tool_result(payload),
        EVENT_TYPE_SESSION_IDLE => vec![ProcessEvent::Done],
        EVENT_TYPE_SESSION_ERROR => {
            let message = extract_error_message(payload);
            vec![ProcessEvent::Error { message }]
        }
        _ => vec![],
    }
}

/// Extracts the tool name, input and output from a `part` object (used by both
/// top-level `tool_use` events and `message.part.updated` events).
fn map_tool_use_part(part: &Value) -> Vec<ProcessEvent> {
    let mut events = Vec::new();

    let name = part
        .get(KEY_TOOL)
        .or_else(|| part.get(KEY_NAME))
        .or_else(|| part.get(KEY_TOOL_NAME))
        .or_else(|| part.get(KEY_TOOL_NAME_ALT))
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_UNKNOWN)
        .to_string();

    // callID 在同一工具调用的 running/completed 更新间保持稳定，是 ToolUse 与
    // ToolResult 精确关联的关键；旧格式 part 没有 callID 时兜底使用 part.id。
    let id = part
        .get(KEY_CALL_ID)
        .or_else(|| part.get(KEY_ID))
        .and_then(|v| v.as_str())
        .map(String::from);

    let state = part.get(KEY_STATE);
    let input = part
        .get(KEY_INPUT)
        .or_else(|| state.and_then(|s| s.get(KEY_INPUT)))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let output = part
        .get(KEY_OUTPUT)
        .or_else(|| state.and_then(|s| s.get(KEY_OUTPUT)))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let status = part
        .get(KEY_STATUS)
        .or_else(|| state.and_then(|s| s.get(KEY_STATUS)))
        .and_then(|v| v.as_str());

    let is_completed = status == Some("completed") || output.is_some();
    let has_meaningful_input = !input.is_null() && input != json!({});

    // 只有携带有效输入且未完成的工具调用才发送 ToolUse；
    // 空输入的通知会生成无意义的“空 args”卡片，直接丢弃。
    if has_meaningful_input && !is_completed {
        events.push(ProcessEvent::ToolUse {
            name: name.clone(),
            input,
            id: id.clone(),
        });
    }

    if let Some(output) = output {
        let failed = status == Some("failed") || status == Some("error");
        events.push(ProcessEvent::ToolResult {
            name,
            result: output,
            failed,
            id,
        });
    }

    events
}

/// Maps a top-level `tool_use` event to one or more `ProcessEvent`s.
fn map_tool_use(payload: &Value) -> Vec<ProcessEvent> {
    let mut events = Vec::new();

    // Legacy format: {"type":"tool_use","name":"read_file","args":{"path":"..."}}
    if let Some(name) = payload.get(KEY_NAME).and_then(|v| v.as_str()) {
        let input = payload
            .get(KEY_ARGS)
            .or_else(|| payload.get(KEY_INPUT))
            .cloned()
            .unwrap_or_else(|| json!({}));
        // legacy 格式可能没有调用 id 字段，缺失时为 None。
        let id = payload
            .get(KEY_CALL_ID)
            .or_else(|| payload.get(KEY_ID))
            .and_then(|v| v.as_str())
            .map(String::from);
        if !input.is_null() && input != json!({}) {
            events.push(ProcessEvent::ToolUse {
                name: name.to_string(),
                input,
                id,
            });
        }
    }

    // Native OpenCode format: {"type":"tool_use","part":{"type":"tool","tool":"write","state":{...}}}
    if let Some(part) = payload.get(KEY_PART) {
        events.extend(map_tool_use_part(part));
    }

    events
}

/// Maps a top-level `tool_result` event to a `ProcessEvent::ToolResult`.
fn map_tool_result(payload: &Value) -> Vec<ProcessEvent> {
    let name = payload
        .get(KEY_NAME)
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_UNKNOWN)
        .to_string();
    let result = payload
        .get(KEY_RESULT)
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_EMPTY)
        .to_string();
    let failed = payload
        .get(KEY_FAILED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // legacy 顶层 tool_result 同样透传调用 id（若有）。
    let id = payload
        .get(KEY_CALL_ID)
        .or_else(|| payload.get(KEY_ID))
        .and_then(|v| v.as_str())
        .map(String::from);
    vec![ProcessEvent::ToolResult {
        name,
        result,
        failed,
        id,
    }]
}

/// 从 session.error 的 JSON payload 中提取有意义的错误信息。
///
/// OpenCode 的 session.error 格式通常为：
/// ```json
/// { "type": "session.error", "message": "...", "error": { "code": "...", "message": "..." } }
/// ```
///
/// 提取优先级：
/// 1. `error.message` — 嵌套的具体错误消息
/// 2. `message` — 顶层错误消息
/// 3. `error` 字符串 — 如果 error 不是对象而是字符串值
/// 4. 整段 JSON 原文作为兜底
fn extract_error_message(payload: &Value) -> String {
    if let Some(msg) = payload
        .get(KEY_ERROR)
        .and_then(|e| e.get(KEY_MESSAGE))
        .and_then(|v| v.as_str())
    {
        if !msg.is_empty() {
            return msg.to_string();
        }
    }
    if let Some(msg) = payload.get(KEY_MESSAGE).and_then(|v| v.as_str()) {
        if !msg.is_empty() {
            return msg.to_string();
        }
    }
    if let Some(msg) = payload.get(KEY_ERROR).and_then(|v| v.as_str()) {
        if !msg.is_empty() {
            return msg.to_string();
        }
    }
    payload.to_string()
}

/// Interaction request extracted from an OpenCode message response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionRequest {
    Question { questions: Vec<QuestionItem> },
    Permission { tool_name: String, action: String },
    TodoWrite { todos: Vec<TodoItem> },
}

/// Converts an [`InteractionRequest`] into a [`ProcessEvent`].
pub fn map_interaction(interaction: &InteractionRequest) -> ProcessEvent {
    match interaction.clone() {
        InteractionRequest::Question { questions } => ProcessEvent::Question { questions },
        InteractionRequest::Permission { tool_name, action } => {
            ProcessEvent::Permission { tool_name, action }
        }
        InteractionRequest::TodoWrite { todos } => ProcessEvent::TodoWrite { todos },
    }
}

/// Detect a question interaction from a tool_use input payload.
///
/// This is used in the SSE relay loop to recognize the opencode `question` tool
/// while a run is still in progress, so the gatewayd run can be paused and the
/// frontend prompted for a response.
pub fn detect_question_tool_input(input: &Value) -> Option<InteractionRequest> {
    parse_question(input)
}

fn parse_question(input: &Value) -> Option<InteractionRequest> {
    let questions_value = input.get(KEY_QUESTIONS).cloned().unwrap_or(Value::Null);

    // 优先兼容旧格式：questions 数组元素为 {id, text}。
    if let Ok(questions) = serde_json::from_value::<Vec<QuestionItem>>(questions_value.clone()) {
        if !questions.is_empty() {
            return Some(InteractionRequest::Question { questions });
        }
    }

    // 新版 opencode question 工具格式：questions 数组元素为
    // {header, question, options: [{label, description}]}。
    let arr = questions_value.as_array()?;
    let mut questions = Vec::new();
    for q in arr {
        let text = q
            .get(KEY_HEADER)
            .or_else(|| q.get(KEY_QUESTION))
            .or_else(|| q.get(KEY_TEXT))
            .and_then(|v| v.as_str())?;
        let options = q.get(KEY_OPTIONS).and_then(|v| v.as_array()).map(|opts| {
            opts.iter().filter_map(parse_question_option).collect::<Vec<_>>()
        });
        // id 沿用现有取值逻辑：第一个选项的 label，无选项时兜底 "confirm"。
        let id = options
            .as_ref()
            .and_then(|opts| opts.first())
            .map(|opt| opt.label.clone())
            .unwrap_or_else(|| DEFAULT_QUESTION_ID.to_string());
        questions.push(QuestionItem {
            id,
            text: text.to_string(),
            options,
        });
    }
    if questions.is_empty() {
        return None;
    }
    Some(InteractionRequest::Question { questions })
}

/// 解析 question 工具的单个选项 {label, description}；缺少 label 的项被丢弃。
fn parse_question_option(value: &Value) -> Option<QuestionOption> {
    let label = value.get(KEY_LABEL)?.as_str()?;
    let description = value
        .get(KEY_DESCRIPTION)
        .and_then(|v| v.as_str())
        .map(String::from);
    Some(QuestionOption {
        label: label.to_string(),
        description,
    })
}

fn parse_todo_write(input: &Value) -> Option<InteractionRequest> {
    let todos_value = input.get(KEY_TODOS).cloned().unwrap_or(Value::Null);
    let todos = serde_json::from_value::<Vec<TodoItem>>(todos_value).ok()?;
    Some(InteractionRequest::TodoWrite { todos })
}

fn parse_permission(part: &Value) -> Option<InteractionRequest> {
    let tool_name = part
        .get(KEY_TOOL_NAME)
        .or_else(|| part.get(KEY_TOOL_NAME_ALT))
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_UNKNOWN);
    let action = part
        .get(KEY_ACTION)
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_EMPTY);
    Some(InteractionRequest::Permission {
        tool_name: tool_name.to_string(),
        action: action.to_string(),
    })
}

/// Scans OpenCode message parts and returns the first detected interaction.
pub fn detect_interaction_from_parts(parts: &[Value]) -> Option<InteractionRequest> {
    for part in parts {
        let part_type = part.get(KEY_TYPE).and_then(|v| v.as_str());
        match part_type {
            Some(PART_TYPE_TOOL_USE) => {
                let input = part.get(KEY_INPUT)?;
                if let Some(request) = parse_question(input) {
                    return Some(request);
                }
                if let Some(request) = parse_todo_write(input) {
                    return Some(request);
                }
            }
            Some(PART_TYPE_PERMISSION) | Some(PART_TYPE_ASK_PERMISSION) => {
                if let Some(request) = parse_permission(part) {
                    return Some(request);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extracts the OpenCode session id from an SSE payload.
pub fn extract_session_id(payload: &Value) -> Option<String> {
    payload
        .get(KEY_PROPERTIES)
        .and_then(|p| p.get(KEY_SESSION_ID))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_map_message_part_delta() {
        let payload = json!({
            "type": "message.part.delta",
            "properties": { "delta": "hello" }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::TextDelta {
                text: "hello".into()
            }
        );
    }

    #[test]
    fn test_map_thinking_with_content() {
        let payload = json!({
            "type": "thinking",
            "content": "planning"
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Thinking {
                content: "planning".into()
            }
        );
    }

    #[test]
    fn test_map_thinking_with_text_fallback() {
        let payload = json!({
            "type": "thinking",
            "text": "reasoning"
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Thinking {
                content: "reasoning".into()
            }
        );
    }

    #[test]
    fn test_map_step_start() {
        let payload = json!({
            "type": "message.part.updated",
            "properties": {
                "part": {
                    "type": "step-start",
                    "text": "step one"
                }
            }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Thinking {
                content: "step one".into()
            }
        );
    }

    #[test]
    fn test_map_step_start_non_step_returns_empty() {
        let payload = json!({
            "type": "message.part.updated",
            "properties": {
                "part": { "type": "text", "text": "plain" }
            }
        });
        assert!(map_opencode_sse(&payload).is_empty());
    }

    #[test]
    fn test_map_tool_use_legacy() {
        let payload = json!({
            "type": "tool_use",
            "name": "read_file",
            "args": { "path": "/tmp/a.txt" }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolUse {
                name: "read_file".into(),
                input: json!({ "path": "/tmp/a.txt" }),
                id: None,
            }
        );
    }

    #[test]
    fn test_map_tool_use_legacy_with_id() {
        // legacy 格式携带 id 字段时应透传。
        let payload = json!({
            "type": "tool_use",
            "name": "read_file",
            "id": "call-legacy-1",
            "args": { "path": "/tmp/a.txt" }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolUse {
                name: "read_file".into(),
                input: json!({ "path": "/tmp/a.txt" }),
                id: Some("call-legacy-1".into()),
            }
        );
    }

    #[test]
    fn test_map_tool_use_opencode_completed() {
        let payload = json!({
            "type": "tool_use",
            "part": {
                "type": "tool",
                "tool": "write",
                "callID": "call-write-1",
                "state": {
                    "status": "completed",
                    "output": "Wrote file.",
                    "input": { "path": "/tmp/a.txt", "content": "hello" }
                }
            }
        });
        let events = map_opencode_sse(&payload);
        // 已完成的工具调用只产生 ToolResult，避免重复卡片
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolResult {
                name: "write".into(),
                result: "Wrote file.".into(),
                failed: false,
                id: Some("call-write-1".into()),
            }
        );
    }

    #[test]
    fn test_map_tool_use_opencode_running() {
        let payload = json!({
            "type": "tool_use",
            "part": {
                "type": "tool",
                "tool": "write",
                "callID": "call-write-1",
                "state": {
                    "status": "in_progress",
                    "input": { "path": "/tmp/a.txt", "content": "hello" }
                }
            }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolUse {
                name: "write".into(),
                input: json!({ "path": "/tmp/a.txt", "content": "hello" }),
                id: Some("call-write-1".into()),
            }
        );
    }

    #[test]
    fn test_map_tool_use_part_updated() {
        let payload = json!({
            "type": "message.part.updated",
            "properties": {
                "part": {
                    "type": "tool",
                    "tool": "write",
                    "callID": "call-write-2",
                    "state": {
                        "status": "completed",
                        "output": "Wrote file.",
                        "input": { "path": "/tmp/a.txt" }
                    }
                }
            }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolResult {
                name: "write".into(),
                result: "Wrote file.".into(),
                failed: false,
                id: Some("call-write-2".into()),
            }
        );
    }

    #[test]
    fn test_map_tool_use_part_id_fallback() {
        // 无 callID 时兜底使用 part.id。
        let payload = json!({
            "type": "message.part.updated",
            "properties": {
                "part": {
                    "type": "tool",
                    "id": "part-1",
                    "tool": "write",
                    "state": {
                        "status": "in_progress",
                        "input": { "path": "/tmp/a.txt" }
                    }
                }
            }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolUse {
                name: "write".into(),
                input: json!({ "path": "/tmp/a.txt" }),
                id: Some("part-1".into()),
            }
        );
    }

    #[test]
    fn test_parallel_tool_results_correlate_by_call_id() {
        // 并行工具调用：两个 ToolUse 先后 START，结果逆序返回时，
        // 每个 ToolResult 必须携带与各自 ToolUse 相同的 callID，
        // 下游据此精确关联而不会张冠李戴。
        let running = |call_id: &str, tool: &str, input: Value| {
            json!({
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "type": "tool",
                        "tool": tool,
                        "callID": call_id,
                        "state": { "status": "in_progress", "input": input }
                    }
                }
            })
        };
        let completed = |call_id: &str, tool: &str, output: &str| {
            json!({
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "type": "tool",
                        "tool": tool,
                        "callID": call_id,
                        "state": { "status": "completed", "output": output, "input": {} }
                    }
                }
            })
        };

        let events_a = map_opencode_sse(&running("call-A", "bash", json!({ "command": "ls" })));
        let events_b = map_opencode_sse(&running("call-B", "glob", json!({ "pattern": "*.rs" })));
        assert_eq!(events_a.len(), 1);
        assert_eq!(events_b.len(), 1);
        assert!(
            matches!(&events_a[0], ProcessEvent::ToolUse { id, .. } if id.as_deref() == Some("call-A"))
        );
        assert!(
            matches!(&events_b[0], ProcessEvent::ToolUse { id, .. } if id.as_deref() == Some("call-B"))
        );

        // 结果逆序返回：先 B 后 A。
        let results_b = map_opencode_sse(&completed("call-B", "glob", "no files"));
        let results_a = map_opencode_sse(&completed("call-A", "bash", "file list"));
        assert_eq!(results_b.len(), 1);
        assert_eq!(results_a.len(), 1);
        assert!(
            matches!(&results_b[0], ProcessEvent::ToolResult { id, name, .. }
                if id.as_deref() == Some("call-B") && name == "glob"),
            "B 的结果必须携带 call-B: {:?}",
            results_b[0]
        );
        assert!(
            matches!(&results_a[0], ProcessEvent::ToolResult { id, name, .. }
                if id.as_deref() == Some("call-A") && name == "bash"),
            "A 的结果必须携带 call-A: {:?}",
            results_a[0]
        );
    }

    #[test]
    fn test_map_tool_use_empty_input_skipped() {
        let payload = json!({
            "type": "message.part.updated",
            "properties": {
                "part": {
                    "type": "tool",
                    "tool": "write",
                    "state": { "status": "in_progress" }
                }
            }
        });
        let events = map_opencode_sse(&payload);
        assert!(events.is_empty(), "空输入且未完成的工具调用不应产生事件");
    }

    #[test]
    fn test_map_tool_result() {
        let payload = json!({
            "type": "tool_result",
            "name": "read_file",
            "result": "ok",
            "failed": true
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::ToolResult {
                name: "read_file".into(),
                result: "ok".into(),
                failed: true,
                id: None,
            }
        );
    }

    #[test]
    fn test_map_session_idle() {
        let payload = json!({ "type": "session.idle" });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], ProcessEvent::Done);
    }

    #[test]
    fn test_map_session_error() {
        // 场景1：顶层 message 字段
        let payload = json!({ "type": "session.error", "message": "boom" });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Error {
                message: "boom".to_string()
            }
        );

        // 场景2：嵌套 error.message 优先级更高
        let payload = json!({
            "type": "session.error",
            "message": "外层消息",
            "error": { "code": "api_key_invalid", "message": "API 密钥无效" }
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Error {
                message: "API 密钥无效".to_string()
            }
        );

        // 场景3：error 为字符串值
        let payload = json!({
            "type": "session.error",
            "error": "网络连接失败"
        });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Error {
                message: "网络连接失败".to_string()
            }
        );

        // 场景4：无 message/error 字段，回退到完整 JSON
        let payload = json!({ "type": "session.error", "raw": "unknown error" });
        let events = map_opencode_sse(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ProcessEvent::Error {
                message: payload.to_string()
            }
        );
    }

    #[test]
    fn test_map_unknown_event_returns_empty() {
        let payload = json!({ "type": "custom" });
        assert!(map_opencode_sse(&payload).is_empty());
    }

    #[test]
    fn test_extract_session_id() {
        let payload = json!({
            "properties": { "sessionID": "sess-1" }
        });
        assert_eq!(extract_session_id(&payload), Some("sess-1".into()));
    }

    #[test]
    fn test_map_interaction_question() {
        let req = InteractionRequest::Question {
            questions: vec![QuestionItem {
                id: "q1".into(),
                text: "ok?".into(),
                options: None,
            }],
        };
        let ev = map_interaction(&req);
        assert_eq!(
            ev,
            ProcessEvent::Question {
                questions: vec![QuestionItem {
                    id: "q1".into(),
                    text: "ok?".into(),
                    options: None,
                }]
            }
        );
    }

    #[test]
    fn test_parse_question_preserves_options() {
        // 新版 question 工具格式：options 数组必须完整透传到 QuestionItem。
        let input = json!({
            "questions": [{
                "header": "是否继续？",
                "question": "是否继续执行重构？",
                "options": [
                    { "label": "继续", "description": "应用全部改动" },
                    { "label": "取消" }
                ],
                "multiple": false
            }]
        });
        let interaction = detect_question_tool_input(&input).unwrap();
        let InteractionRequest::Question { questions } = interaction else {
            panic!("expected Question interaction");
        };
        assert_eq!(questions.len(), 1);
        // id/text 取值逻辑保持不变：id 取第一个选项的 label，text 优先 header。
        assert_eq!(questions[0].id, "继续");
        assert_eq!(questions[0].text, "是否继续？");
        let options = questions[0].options.as_ref().expect("options 应被保留");
        assert_eq!(
            options,
            &vec![
                QuestionOption {
                    label: "继续".into(),
                    description: Some("应用全部改动".into()),
                },
                QuestionOption {
                    label: "取消".into(),
                    description: None,
                },
            ]
        );
    }

    #[test]
    fn test_parse_question_legacy_format_without_options() {
        // 旧格式 {id, text} 仍能解析，options 为 None。
        let input = json!({
            "questions": [{ "id": "q1", "text": "ok?" }]
        });
        let interaction = detect_question_tool_input(&input).unwrap();
        let InteractionRequest::Question { questions } = interaction else {
            panic!("expected Question interaction");
        };
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].id, "q1");
        assert_eq!(questions[0].options, None);
    }

    #[test]
    fn test_map_interaction_permission() {
        let req = InteractionRequest::Permission {
            tool_name: "bash".into(),
            action: "run".into(),
        };
        let ev = map_interaction(&req);
        assert_eq!(
            ev,
            ProcessEvent::Permission {
                tool_name: "bash".into(),
                action: "run".into()
            }
        );
    }

    #[test]
    fn test_detect_question_from_parts() {
        let parts = vec![json!({
            "type": "tool_use",
            "toolName": "question",
            "input": {
                "questions": [{ "id": "q1", "text": "ok?" }]
            }
        })];
        let interaction = detect_interaction_from_parts(&parts).unwrap();
        assert!(matches!(interaction, InteractionRequest::Question { .. }));
    }

    #[test]
    fn test_detect_permission_from_parts() {
        let parts = vec![json!({
            "type": "permission",
            "toolName": "write",
            "action": "create file"
        })];
        let interaction = detect_interaction_from_parts(&parts).unwrap();
        assert!(matches!(
            interaction,
            InteractionRequest::Permission { tool_name, action }
            if tool_name == "write" && action == "create file"
        ));
    }
}
