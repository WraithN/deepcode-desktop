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
        EVENT_TYPE_SESSION_ERROR => Some(ProcessEvent::Error {
            message: payload.to_string(),
        }),
        _ => None,
    }
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
        let payload = json!({ "type": "session.error", "message": "boom" });
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
