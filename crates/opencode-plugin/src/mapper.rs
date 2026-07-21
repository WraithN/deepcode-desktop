use agent_core::process::event::{ProcessEvent, QuestionItem, TodoItem};
use serde_json::Value;

const KEY_TYPE: &str = "type";
const KEY_PROPERTIES: &str = "properties";
const KEY_DELTA: &str = "delta";
const KEY_CONTENT: &str = "content";
const KEY_TEXT: &str = "text";
const KEY_PART: &str = "part";
const KEY_SESSION_ID: &str = "sessionID";
const KEY_INPUT: &str = "input";
const KEY_QUESTIONS: &str = "questions";
const KEY_TODOS: &str = "todos";
const KEY_MESSAGE: &str = "message";
const KEY_ERROR: &str = "error";
const KEY_TOOL_NAME: &str = "toolName";
const KEY_TOOL_NAME_ALT: &str = "tool_name";
const KEY_ACTION: &str = "action";

const EVENT_TYPE_MESSAGE_PART_DELTA: &str = "message.part.delta";
const EVENT_TYPE_THINKING: &str = "thinking";
const EVENT_TYPE_MESSAGE_PART_UPDATED: &str = "message.part.updated";
const EVENT_TYPE_SESSION_IDLE: &str = "session.idle";
const EVENT_TYPE_SESSION_ERROR: &str = "session.error";

const PART_TYPE_TOOL_USE: &str = "tool_use";
const PART_TYPE_PERMISSION: &str = "permission";
const PART_TYPE_ASK_PERMISSION: &str = "ask_permission";
const STEP_TYPE_STEP_START: &str = "step-start";

const DEFAULT_UNKNOWN: &str = "unknown";
const DEFAULT_EMPTY: &str = "";

/// Maps an OpenCode SSE JSON payload to a unified [`ProcessEvent`].
pub fn map_opencode_sse(payload: &Value) -> Option<ProcessEvent> {
    let event_type = payload
        .get(KEY_TYPE)
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_UNKNOWN);

    match event_type {
        EVENT_TYPE_MESSAGE_PART_DELTA => {
            let delta = payload
                .get(KEY_PROPERTIES)?
                .get(KEY_DELTA)?
                .as_str()?;
            Some(ProcessEvent::TextDelta { text: delta.into() })
        }
        EVENT_TYPE_THINKING => {
            let content = payload
                .get(KEY_CONTENT)
                .or_else(|| payload.get(KEY_TEXT))
                .and_then(|v| v.as_str())
                .unwrap_or(DEFAULT_EMPTY);
            Some(ProcessEvent::Thinking { content: content.into() })
        }
        EVENT_TYPE_MESSAGE_PART_UPDATED => {
            let part = payload.get(KEY_PROPERTIES)?.get(KEY_PART)?;
            if part.get(KEY_TYPE).and_then(|v| v.as_str()) == Some(STEP_TYPE_STEP_START) {
                let text = part.get(KEY_TEXT).and_then(|v| v.as_str()).unwrap_or(DEFAULT_EMPTY);
                Some(ProcessEvent::Thinking { content: text.into() })
            } else {
                None
            }
        }
        EVENT_TYPE_SESSION_IDLE => Some(ProcessEvent::Done),
        EVENT_TYPE_SESSION_ERROR => {
            let message = extract_error_message(payload);
            Some(ProcessEvent::Error { message })
        }
        _ => None,
    }
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
    if let Some(msg) = payload.get(KEY_ERROR).and_then(|e| e.get(KEY_MESSAGE)).and_then(|v| v.as_str()) {
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

fn parse_question(input: &Value) -> Option<InteractionRequest> {
    let questions_value = input.get(KEY_QUESTIONS).cloned().unwrap_or(Value::Null);
    let questions = serde_json::from_value::<Vec<QuestionItem>>(questions_value).ok()?;
    Some(InteractionRequest::Question { questions })
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
    let action = part.get(KEY_ACTION).and_then(|v| v.as_str()).unwrap_or(DEFAULT_EMPTY);
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
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(ev, ProcessEvent::TextDelta { text: "hello".into() });
    }

    #[test]
    fn test_map_thinking_with_content() {
        let payload = json!({
            "type": "thinking",
            "content": "planning"
        });
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(ev, ProcessEvent::Thinking { content: "planning".into() });
    }

    #[test]
    fn test_map_thinking_with_text_fallback() {
        let payload = json!({
            "type": "thinking",
            "text": "reasoning"
        });
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(ev, ProcessEvent::Thinking { content: "reasoning".into() });
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
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(ev, ProcessEvent::Thinking { content: "step one".into() });
    }

    #[test]
    fn test_map_step_start_non_step_returns_none() {
        let payload = json!({
            "type": "message.part.updated",
            "properties": {
                "part": { "type": "text", "text": "plain" }
            }
        });
        assert!(map_opencode_sse(&payload).is_none());
    }

    #[test]
    fn test_map_session_idle() {
        let payload = json!({ "type": "session.idle" });
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(ev, ProcessEvent::Done);
    }

    #[test]
    fn test_map_session_error() {
        // 场景1：顶层 message 字段
        let payload = json!({ "type": "session.error", "message": "boom" });
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(
            ev,
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
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(
            ev,
            ProcessEvent::Error {
                message: "API 密钥无效".to_string()
            }
        );

        // 场景3：error 为字符串值
        let payload = json!({
            "type": "session.error",
            "error": "网络连接失败"
        });
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(
            ev,
            ProcessEvent::Error {
                message: "网络连接失败".to_string()
            }
        );

        // 场景4：无 message/error 字段，回退到完整 JSON
        let payload = json!({ "type": "session.error", "raw": "unknown error" });
        let ev = map_opencode_sse(&payload).unwrap();
        assert_eq!(
            ev,
            ProcessEvent::Error {
                message: payload.to_string()
            }
        );
    }

    #[test]
    fn test_map_unknown_event_returns_none() {
        let payload = json!({ "type": "custom" });
        assert!(map_opencode_sse(&payload).is_none());
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
            }],
        };
        let ev = map_interaction(&req);
        assert_eq!(
            ev,
            ProcessEvent::Question {
                questions: vec![QuestionItem {
                    id: "q1".into(),
                    text: "ok?".into()
                }]
            }
        );
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
