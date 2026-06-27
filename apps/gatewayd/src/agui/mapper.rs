use crate::agui::types::{BaseEvent, Event};
use serde_json::Value;

const METHOD_TOKEN: &str = "agent.token";
const METHOD_THINKING: &str = "agent.thinking";
const METHOD_PERMISSION: &str = "agent.permission";
const METHOD_QUESTION: &str = "agent.question";
const METHOD_TODO_WRITE: &str = "agent.todowrite";
const METHOD_DONE: &str = "agent.done";
const METHOD_ERROR: &str = "agent.error";
const METHOD_STATUS_CHANGED: &str = "agent:status_changed";
const METHOD_SESSION_LOG: &str = "session.log";

const KEY_TYPE: &str = "type";
const KEY_CONTENT: &str = "content";
const KEY_TEXT: &str = "text";
const KEY_TOOL_NAME: &str = "toolName";
const KEY_FAILED: &str = "failed";
const KEY_MESSAGE: &str = "message";

/// Per-run state used to turn discrete JSON-RPC notifications into
/// AG-UI Start/Content/End event sequences.
#[derive(Debug, Default, Clone)]
pub struct AguiMapper {
    current_message_id: Option<String>,
    current_tool_call_id: Option<String>,
}

impl AguiMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map(&mut self, method: &str, payload: &Value) -> Vec<Event> {
        let base = BaseEvent {
            timestamp: Some(now()),
            raw_event: None,
        };
        match method {
            METHOD_TOKEN => self.map_token(base, payload),
            METHOD_THINKING => self.map_thinking(base, payload),
            METHOD_PERMISSION | METHOD_QUESTION | METHOD_TODO_WRITE => vec![Event::Custom {
                base,
                name: method.to_string(),
                value: payload.clone(),
            }],
            METHOD_DONE => self.map_done(base),
            METHOD_ERROR => self.map_error(base, payload),
            METHOD_STATUS_CHANGED => vec![Event::Custom {
                base,
                name: "status_changed".to_string(),
                value: payload.clone(),
            }],
            METHOD_SESSION_LOG => vec![Event::Raw {
                base,
                event: payload.clone(),
                source: Some("session-log".to_string()),
            }],
            _ => vec![],
        }
    }

    fn map_token(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let delta = payload
            .get(KEY_TEXT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if delta.is_empty() {
            return vec![];
        }

        let mut events = Vec::new();
        if self.current_message_id.is_none() {
            let id = new_id();
            self.current_message_id = Some(id.clone());
            events.push(Event::TextMessageStart {
                base: base.clone(),
                message_id: id,
                role: "assistant".to_string(),
            });
        }

        let message_id = self.current_message_id.clone().unwrap();
        events.push(Event::TextMessageContent {
            base,
            message_id,
            delta,
        });
        events
    }

    fn map_thinking(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let ev_type = payload.get(KEY_TYPE).and_then(|v| v.as_str());
        match ev_type {
            Some("tool_use") => self.map_tool_use(base, payload),
            Some("tool_result") => self.map_tool_result(base, payload),
            _ => {
                let delta = payload
                    .get(KEY_CONTENT)
                    .or_else(|| payload.get(KEY_TEXT))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if delta.is_empty() {
                    return vec![];
                }
                vec![
                    Event::ThinkingTextMessageStart { base: base.clone() },
                    Event::ThinkingTextMessageContent { base, delta },
                    Event::ThinkingTextMessageEnd {
                        base: BaseEvent {
                            timestamp: Some(now()),
                            raw_event: None,
                        },
                    },
                ]
            }
        }
    }

    fn map_tool_use(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let tool_call_id = new_id();
        self.current_tool_call_id = Some(tool_call_id.clone());
        let tool_call_name = payload
            .get(KEY_TOOL_NAME)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let delta = payload
            .get(KEY_CONTENT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        vec![
            Event::ToolCallStart {
                base: base.clone(),
                tool_call_id: tool_call_id.clone(),
                tool_call_name,
            },
            Event::ToolCallArgs {
                base,
                tool_call_id,
                delta,
            },
        ]
    }

    fn map_tool_result(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let content = payload
            .get(KEY_CONTENT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_call_id = self.current_tool_call_id.clone().unwrap_or_else(new_id);
        let message_id = self.current_message_id.clone().unwrap_or_else(new_id);
        vec![Event::ToolCallResult {
            base,
            tool_call_id,
            message_id,
            content,
        }]
    }

    fn map_done(&mut self, base: BaseEvent) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(id) = self.current_message_id.take() {
            events.push(Event::TextMessageEnd {
                base: base.clone(),
                message_id: id,
            });
        }
        self.current_tool_call_id = None;
        events
    }

    fn map_error(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let mut events = self.map_done(base.clone());
        let message = payload
            .get(KEY_MESSAGE)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error")
            .to_string();
        events.push(Event::RunError {
            base,
            message,
            code: None,
        });
        events
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agui::types::Event;
    use serde_json::json;

    #[test]
    fn test_map_token_sequence() {
        let mut mapper = AguiMapper::new();
        let events = mapper.map(METHOD_TOKEN, &json!({ "text": "hello" }));
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], Event::TextMessageStart { .. }));
        assert!(matches!(events[1], Event::TextMessageContent { .. }));

        let events = mapper.map(METHOD_TOKEN, &json!({ "text": " world" }));
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::TextMessageContent { .. }));

        let events = mapper.map(METHOD_DONE, &json!({}));
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::TextMessageEnd { .. }));
    }

    #[test]
    fn test_map_thinking() {
        let mut mapper = AguiMapper::new();
        let events = mapper.map(
            METHOD_THINKING,
            &json!({ "content": "planning", "type": "thinking" }),
        );
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], Event::ThinkingTextMessageStart { .. }));
        assert!(matches!(
            events[1],
            Event::ThinkingTextMessageContent { .. }
        ));
        assert!(matches!(events[2], Event::ThinkingTextMessageEnd { .. }));
    }

    #[test]
    fn test_map_error_closes_message() {
        let mut mapper = AguiMapper::new();
        mapper.map(METHOD_TOKEN, &json!({ "text": "x" }));
        let events = mapper.map(METHOD_ERROR, &json!({ "message": "boom" }));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TextMessageEnd { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, Event::RunError { .. })));
    }
}
