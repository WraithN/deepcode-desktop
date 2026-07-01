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
                // AG-UI client 要求 thinking 事件必须以 THINKING_START 开始、
                // THINKING_END 结束，中间包裹 THINKING_TEXT_MESSAGE_* 序列。
                vec![
                    Event::ThinkingStart { base: base.clone() },
                    Event::ThinkingTextMessageStart { base: base.clone() },
                    Event::ThinkingTextMessageContent {
                        base: base.clone(),
                        delta,
                    },
                    Event::ThinkingTextMessageEnd { base: base.clone() },
                    Event::ThinkingEnd {
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
        let raw_delta = payload
            .get(KEY_CONTENT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // claude-plugin 把 tool 名称和参数拼成 "WebSearch {\"query\":\"...\"}" 字符串放在 content 里，
        // 但顶层的 toolName 经常为 "unknown"。这里从 content 中解析出真实工具名和 JSON 参数，
        // 这样前端才能正确展示工具卡片和参数。
        let (tool_call_name, args_delta) = parse_tool_call_delta(
            payload.get(KEY_TOOL_NAME).and_then(|v| v.as_str()),
            &raw_delta,
        );

        vec![
            Event::ToolCallStart {
                base: base.clone(),
                tool_call_id: tool_call_id.clone(),
                tool_call_name,
            },
            Event::ToolCallArgs {
                base,
                tool_call_id,
                delta: args_delta,
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

/// 从 claude-plugin 生成的 tool_use content 字符串中解析工具名和参数。
///
/// content 通常为 "WebSearch {\"query\":\"...\"}" 这种 "工具名 JSON" 的格式。
/// 当顶层 toolName 不存在或为 "unknown" 时，从 content 前缀提取工具名；
/// 参数部分取第一个 `{` 或 `[` 开始到字符串末尾的 JSON 片段。
fn parse_tool_call_delta(tool_name: Option<&str>, content: &str) -> (String, String) {
    let fallback_name = tool_name
        .filter(|n| !n.is_empty() && *n != "unknown")
        .map(String::from)
        .unwrap_or_else(|| "工具调用".to_string());

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return (fallback_name, content.to_string());
    }

    // 如果 content 本身就是合法 JSON，直接作为参数，工具名使用 fallback。
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return (fallback_name, trimmed.to_string());
    }

    // 查找 JSON 参数起始位置。
    let json_start = trimmed
        .find('{')
        .or_else(|| trimmed.find('['))
        .unwrap_or(trimmed.len());

    let name_part = trimmed[..json_start].trim();
    let args_part = &trimmed[json_start..];

    let name = if name_part.is_empty() {
        fallback_name
    } else {
        name_part.to_string()
    };

    let args = if args_part.is_empty() {
        content.to_string()
    } else {
        args_part.to_string()
    };

    (name, args)
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
