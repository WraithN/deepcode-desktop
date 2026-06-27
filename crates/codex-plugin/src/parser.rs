use agent_core::process::event::ProcessEvent;
use serde_json::Value;

use crate::constants::*;

/// Parses a Codex app-server JSON-RPC notification payload into a normalized
/// `ProcessEvent` that the frontend event mapper can consume.
pub fn parse_codex_notification(value: &Value) -> Option<ProcessEvent> {
    let method = value.get(KEY_METHOD).and_then(|v| v.as_str())?;
    let params = value.get(KEY_PARAMS).cloned().unwrap_or(Value::Null);

    match method {
        EVENT_TURN_STARTED => Some(ProcessEvent::Thinking {
            content: "turn started".into(),
        }),
        EVENT_TURN_COMPLETED => Some(ProcessEvent::Done),
        EVENT_ITEM_AGENT_MESSAGE_DELTA => parse_agent_message_delta(&params),
        EVENT_ITEM_COMMAND_EXECUTION_OUTPUT_DELTA => parse_command_output_delta(&params),
        EVENT_ITEM_FILE_CHANGE => Some(ProcessEvent::Thinking {
            content: "file changed".into(),
        }),
        EVENT_ERROR => Some(ProcessEvent::Error {
            message: params
                .get(KEY_MESSAGE)
                .and_then(|v| v.as_str())
                .unwrap_or("codex error")
                .into(),
        }),
        _ => None,
    }
}

fn parse_agent_message_delta(params: &Value) -> Option<ProcessEvent> {
    let delta = params.get(KEY_DELTA)?;
    if let Some(text) = delta.get(KEY_TEXT).and_then(|v| v.as_str()) {
        return Some(ProcessEvent::TextDelta { text: text.into() });
    }
    if let Some(content) = delta.get(KEY_CONTENT).and_then(|v| v.as_str()) {
        return Some(ProcessEvent::TextDelta { text: content.into() });
    }
    None
}

fn parse_command_output_delta(params: &Value) -> Option<ProcessEvent> {
    let delta = params.get(KEY_DELTA)?;
    if let Some(text) = delta.get(KEY_TEXT).and_then(|v| v.as_str()) {
        return Some(ProcessEvent::TextDelta { text: text.into() });
    }
    None
}

/// Extracts a thread id from a `thread/start` JSON-RPC response.
pub fn extract_thread_id(value: &Value) -> Option<String> {
    value
        .get(KEY_RESULT)
        .and_then(|r| r.get(KEY_THREAD))
        .and_then(|t| t.get(KEY_THREAD_ID).or_else(|| t.get(KEY_ID)))
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            value
                .get(KEY_RESULT)
                .and_then(|r| r.get(KEY_THREAD_ID).or_else(|| r.get(KEY_ID)))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_agent_message_delta() {
        let value = json!({
            "method": EVENT_ITEM_AGENT_MESSAGE_DELTA,
            "params": { "delta": { "text": "hello" } }
        });
        let event = parse_codex_notification(&value).unwrap();
        assert_eq!(event, ProcessEvent::TextDelta { text: "hello".into() });
    }

    #[test]
    fn test_parse_turn_completed() {
        let value = json!({
            "method": EVENT_TURN_COMPLETED,
            "params": {}
        });
        let event = parse_codex_notification(&value).unwrap();
        assert_eq!(event, ProcessEvent::Done);
    }

    #[test]
    fn test_extract_thread_id() {
        let value = json!({
            "result": {
                "thread": { "thread_id": "t-123" }
            }
        });
        assert_eq!(extract_thread_id(&value), Some("t-123".to_string()));
    }
}
