use agent_core::process::event::ProcessEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const TYPE_SYSTEM: &str = "system";
const TYPE_USER: &str = "user";
const TYPE_ASSISTANT: &str = "assistant";
const TYPE_STREAM_EVENT: &str = "stream_event";
const TYPE_RESULT: &str = "result";
const TYPE_ERROR: &str = "error";
const TYPE_TEXT_DELTA: &str = "text_delta";
const TYPE_THINKING_DELTA: &str = "thinking_delta";
const TYPE_MESSAGE_STOP: &str = "message_stop";
const TYPE_CONTENT_BLOCK_DELTA: &str = "content_block_delta";
const CONTENT_TYPE_TEXT: &str = "text";
const CONTENT_TYPE_THINKING: &str = "thinking";
const CONTENT_TYPE_TOOL_USE: &str = "tool_use";
const CONTENT_TYPE_TOOL_RESULT: &str = "tool_result";
const KEY_TYPE: &str = "type";
const KEY_SUBTYPE: &str = "subtype";
const KEY_MESSAGE: &str = "message";
const KEY_CONTENT: &str = "content";
const KEY_EVENT: &str = "event";
const KEY_DELTA: &str = "delta";
const KEY_TEXT: &str = "text";
const KEY_THINKING: &str = "thinking";
const KEY_RESULT: &str = "result";
const KEY_IS_ERROR: &str = "is_error";
const KEY_SESSION_ID: &str = "session_id";
const KEY_ERROR: &str = "error";
const KEY_MESSAGE_TEXT: &str = "message";
const KEY_NAME: &str = "name";
const KEY_INPUT: &str = "input";
const KEY_IS_ERROR_CONTENT: &str = "is_error";
const KEY_TOOL_USE_RESULT: &str = "tool_use_result";
const KEY_STDOUT: &str = "stdout";
const KEY_STDERR: &str = "stderr";

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
    ToolUse {
        name: String,
        input: Value,
    },
    ToolResult {
        result: String,
        failed: bool,
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
    serde_json::from_value(value.clone())
        .ok()
        .or_else(|| parse_current_claude_value(value))
}

fn parse_current_claude_value(value: &Value) -> Option<ClaudeRawEvent> {
    match value.get(KEY_TYPE)?.as_str()? {
        TYPE_SYSTEM => parse_system_value(value),
        TYPE_USER => parse_user_value(value),
        TYPE_ASSISTANT => parse_assistant_value(value),
        TYPE_STREAM_EVENT => parse_stream_event_value(value),
        TYPE_RESULT => parse_result_value(value),
        TYPE_ERROR => parse_error_value(value),
        _ => None,
    }
}

fn parse_system_value(value: &Value) -> Option<ClaudeRawEvent> {
    Some(ClaudeRawEvent::System {
        subtype: value.get(KEY_SUBTYPE)?.as_str()?.to_string(),
        extra: value.clone(),
    })
}

fn parse_assistant_value(value: &Value) -> Option<ClaudeRawEvent> {
    // Claude 在 --print stream-json 模式下遇到 API 错误时，会返回 type=assistant 事件，
    // 并在顶层携带 error 字段，文本内容即为错误说明（如 "API Error: 402 Insufficient Balance"）。
    // 这种情况下没有 text_delta 流，原有逻辑会忽略文本导致前端空响应，
    // 因此优先将其识别为 Error 事件。
    if value.get(KEY_ERROR).is_some() {
        return parse_error_value(value);
    }

    let message = value.get(KEY_MESSAGE).unwrap_or(value);
    let content = parse_content(message.get(KEY_CONTENT)?)?;
    if let Some(tool_use) = first_tool_use(&content) {
        return Some(tool_use);
    }
    Some(ClaudeRawEvent::Assistant { content })
}

fn parse_user_value(value: &Value) -> Option<ClaudeRawEvent> {
    let message = value.get(KEY_MESSAGE).unwrap_or(value);
    let content = parse_content(message.get(KEY_CONTENT)?)?;
    if let Some(tool_result) = first_tool_result(&content) {
        return Some(tool_result);
    }
    Some(ClaudeRawEvent::User { content })
}

fn first_tool_use(content: &[ClaudeContent]) -> Option<ClaudeRawEvent> {
    content.iter().find_map(|item| match item {
        ClaudeContent::ToolUse { name, input } => Some(ClaudeRawEvent::ToolUse {
            name: name.clone(),
            input: input.clone(),
        }),
        _ => None,
    })
}

fn first_tool_result(content: &[ClaudeContent]) -> Option<ClaudeRawEvent> {
    content.iter().find_map(|item| match item {
        ClaudeContent::ToolResult { result, failed } => Some(ClaudeRawEvent::ToolResult {
            name: result_tool_name(result),
            result: result.clone(),
            failed: Some(*failed),
        }),
        _ => None,
    })
}

fn result_tool_name(result: &str) -> String {
    result
        .trim_start_matches('(')
        .split_whitespace()
        .next()
        .unwrap_or("tool")
        .to_string()
}

fn parse_stream_event_value(value: &Value) -> Option<ClaudeRawEvent> {
    let event = value.get(KEY_EVENT)?;
    let event_type = event.get(KEY_TYPE)?.as_str()?;

    // Claude --include-partial-messages 输出的事件结构：
    // event.type = "content_block_delta", event.delta.type = "text_delta"/"thinking_delta"
    if event_type == TYPE_CONTENT_BLOCK_DELTA {
        let delta = event.get(KEY_DELTA)?;
        return match delta.get(KEY_TYPE)?.as_str()? {
            TYPE_TEXT_DELTA => Some(ClaudeRawEvent::StreamEvent {
                event: ClaudeStreamEvent::TextDelta {
                    delta: ClaudeTextDelta {
                        text: delta.get(KEY_TEXT)?.as_str()?.to_string(),
                    },
                },
            }),
            TYPE_THINKING_DELTA => Some(ClaudeRawEvent::StreamEvent {
                event: ClaudeStreamEvent::ThinkingDelta {
                    delta: ClaudeThinkingDelta {
                        thinking: delta.get(KEY_THINKING)?.as_str()?.to_string(),
                    },
                },
            }),
            _ => None,
        };
    }

    let parsed = match event_type {
        TYPE_TEXT_DELTA => ClaudeStreamEvent::TextDelta {
            delta: ClaudeTextDelta {
                text: event.get(KEY_DELTA)?.get(KEY_TEXT)?.as_str()?.to_string(),
            },
        },
        TYPE_THINKING_DELTA => ClaudeStreamEvent::ThinkingDelta {
            delta: ClaudeThinkingDelta {
                thinking: event.get(KEY_DELTA)?.get(KEY_THINKING)?.as_str()?.to_string(),
            },
        },
        TYPE_MESSAGE_STOP => ClaudeStreamEvent::MessageStop {},
        _ => return None,
    };
    Some(ClaudeRawEvent::StreamEvent { event: parsed })
}

fn parse_result_value(value: &Value) -> Option<ClaudeRawEvent> {
    if value.get(KEY_IS_ERROR).and_then(|v| v.as_bool()).unwrap_or(false) {
        return parse_error_value(value);
    }
    Some(ClaudeRawEvent::Result {
        result: value.get(KEY_RESULT).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        session_id: value.get(KEY_SESSION_ID)?.as_str()?.to_string(),
    })
}

fn parse_error_value(value: &Value) -> Option<ClaudeRawEvent> {
    let message = value
        .get(KEY_ERROR)
        .and_then(|v| v.get(KEY_MESSAGE_TEXT))
        .or_else(|| value.get(KEY_MESSAGE_TEXT))
        .or_else(|| value.get(KEY_RESULT))
        .and_then(|v| v.as_str().map(String::from))
        .or_else(|| extract_text_from_message_content(value))
        .unwrap_or_else(|| "unknown error".to_string());
    Some(ClaudeRawEvent::Error { message })
}

/// 从 assistant/user 消息的 content 数组中提取第一段文本内容，用于错误提示。
fn extract_text_from_message_content(value: &Value) -> Option<String> {
    let message = value.get(KEY_MESSAGE).unwrap_or(value);
    let content = message.get(KEY_CONTENT)?.as_array()?;
    content
        .iter()
        .filter_map(|item| {
            if item.get(KEY_TYPE)?.as_str()? == CONTENT_TYPE_TEXT {
                item.get(KEY_TEXT)?.as_str().map(String::from)
            } else {
                None
            }
        })
        .next()
}

fn parse_content(value: &Value) -> Option<Vec<ClaudeContent>> {
    let items = value.as_array()?;
    Some(items.iter().filter_map(parse_content_item).collect())
}

fn parse_content_item(value: &Value) -> Option<ClaudeContent> {
    match value.get(KEY_TYPE)?.as_str()? {
        CONTENT_TYPE_TEXT => Some(ClaudeContent::Text {
            text: value.get(KEY_TEXT)?.as_str()?.to_string(),
        }),
        CONTENT_TYPE_THINKING => Some(ClaudeContent::Thinking {
            thinking: value.get(KEY_THINKING)?.as_str()?.to_string(),
        }),
        CONTENT_TYPE_TOOL_USE => Some(ClaudeContent::ToolUse {
            name: value.get(KEY_NAME)?.as_str()?.to_string(),
            input: value.get(KEY_INPUT).cloned().unwrap_or(Value::Null),
        }),
        CONTENT_TYPE_TOOL_RESULT => Some(ClaudeContent::ToolResult {
            result: parse_tool_result_text(value),
            failed: value
                .get(KEY_IS_ERROR_CONTENT)
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        _ => None,
    }
}

fn parse_tool_result_text(value: &Value) -> String {
    let content = value.get(KEY_CONTENT).and_then(|v| v.as_str());
    let stdout = value
        .get(KEY_TOOL_USE_RESULT)
        .and_then(|v| v.get(KEY_STDOUT))
        .and_then(|v| v.as_str());
    let stderr = value
        .get(KEY_TOOL_USE_RESULT)
        .and_then(|v| v.get(KEY_STDERR))
        .and_then(|v| v.as_str());
    content
        .or(stdout)
        .or(stderr)
        .unwrap_or_default()
        .to_string()
}

/// Converts a raw Claude event into the normalized `ProcessEvent`.
pub fn to_process_event(raw: &ClaudeRawEvent) -> Option<ProcessEvent> {
    match raw {
        ClaudeRawEvent::StreamEvent { event } => match event {
            ClaudeStreamEvent::TextDelta { delta } => {
                Some(ProcessEvent::TextDelta { text: delta.text.clone() })
            }
            // 过滤 thinking 流式片段：避免每个 token 都产生一组 AG-UI thinking 事件。
            // 前端通过 isRunning 状态展示“思考中...”占位符即可。
            ClaudeStreamEvent::ThinkingDelta { .. } => None,
            ClaudeStreamEvent::MessageStop {} => Some(ProcessEvent::Done),
        },
        // stream-json 模式下文本增量已由 StreamEvent::TextDelta 输出，
        // 忽略 Assistant 事件中的完整文本，避免在流末尾再发一遍重复内容。
        // 保留 thinking 内容作为一次性“思考中...”提示。
        ClaudeRawEvent::Assistant { content } => {
            let thinking: String = content
                .iter()
                .filter_map(|c| match c {
                    ClaudeContent::Thinking { thinking } => Some(thinking.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if !thinking.is_empty() {
                Some(ProcessEvent::Thinking { content: thinking })
            } else {
                None
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
