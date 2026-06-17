use agent_core::process::event::ProcessEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level raw event emitted by the Claude CLI JSON stream.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeRawEvent {
    System {
        subtype: String,
        #[serde(flatten)]
        extra: Value,
    },
    User {
        content: Vec<ClaudeContent>,
    },
    Assistant {
        content: Vec<ClaudeContent>,
    },
    StreamEvent {
        event: ClaudeStreamEvent,
    },
    ToolUse {
        name: String,
        input: Value,
    },
    ToolResult {
        name: String,
        result: String,
        failed: Option<bool>,
    },
    Result {
        result: String,
        session_id: String,
    },
    Error {
        message: String,
    },
}

/// Content block inside a `user` or `assistant` raw event.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeContent {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
}

/// Nested event carried by `ClaudeRawEvent::StreamEvent`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeStreamEvent {
    TextDelta {
        delta: ClaudeTextDelta,
    },
    ThinkingDelta {
        delta: ClaudeThinkingDelta,
    },
    MessageStop {},
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeTextDelta {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeThinkingDelta {
    pub thinking: String,
}

/// Parses a single non-empty JSON line into a raw Claude event.
pub fn parse_claude_line(line: &str) -> Option<ClaudeRawEvent> {
    agent_core::process::parse_json_line(line)
}

/// Parses an already-decoded `serde_json::Value` into a raw Claude event.
pub fn parse_claude_value(value: &Value) -> Option<ClaudeRawEvent> {
    serde_json::from_value(value.clone()).ok()
}

/// Converts a raw Claude event into the normalized `ProcessEvent`.
pub fn to_process_event(raw: &ClaudeRawEvent) -> Option<ProcessEvent> {
    match raw {
        ClaudeRawEvent::StreamEvent { event } => match event {
            ClaudeStreamEvent::TextDelta { delta } => {
                Some(ProcessEvent::TextDelta { text: delta.text.clone() })
            }
            ClaudeStreamEvent::ThinkingDelta { delta } => Some(ProcessEvent::Thinking {
                content: delta.thinking.clone(),
            }),
            ClaudeStreamEvent::MessageStop {} => Some(ProcessEvent::Done),
        },
        ClaudeRawEvent::Assistant { content } => {
            let text: String = content
                .iter()
                .filter_map(|c| match c {
                    ClaudeContent::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() {
                None
            } else {
                Some(ProcessEvent::AssistantMessage { content: text })
            }
        }
        ClaudeRawEvent::ToolUse { name, input } => Some(ProcessEvent::ToolUse {
            name: name.clone(),
            input: input.clone(),
        }),
        ClaudeRawEvent::ToolResult { name, result, failed } => Some(ProcessEvent::ToolResult {
            name: name.clone(),
            result: result.clone(),
            failed: failed.unwrap_or(false),
        }),
        ClaudeRawEvent::Result { .. } => Some(ProcessEvent::Done),
        ClaudeRawEvent::Error { message } => Some(ProcessEvent::Error {
            message: message.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_DELTA_LINE: &str = r#"{"type":"stream_event","event":{"type":"text_delta","delta":{"text":"hello"}}}"#;
    const RESULT_LINE: &str = r#"{"type":"result","result":"ok","session_id":"s-1"}"#;

    #[test]
    fn test_parse_text_delta() {
        let raw = parse_claude_line(TEXT_DELTA_LINE).unwrap();
        let ev = to_process_event(&raw).unwrap();
        assert!(matches!(ev, ProcessEvent::TextDelta { text } if text == "hello"));
    }

    #[test]
    fn test_parse_done() {
        let raw = parse_claude_line(RESULT_LINE).unwrap();
        let ev = to_process_event(&raw).unwrap();
        assert!(matches!(ev, ProcessEvent::Done));
    }
}
