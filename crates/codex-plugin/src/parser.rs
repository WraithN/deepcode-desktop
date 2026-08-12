use agent_core::process::event::ProcessEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::*;

/// A single JSON-RPC message on the Codex app-server wire.
///
/// Codex omits the standard `"jsonrpc":"2.0"` header, so this loose shape
/// handles requests, responses, and notifications.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CodexMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl CodexMessage {
    pub fn is_notification(&self) -> bool {
        self.method.is_some() && self.id.is_none()
    }

    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }

    pub fn response_id(&self) -> Option<i64> {
        self.id.as_ref().and_then(|v| v.as_i64())
    }
}

/// Parses a single non-empty NDJSON line into a generic Codex message.
pub fn parse_codex_line(line: &str) -> Option<CodexMessage> {
    agent_core::process::parse_json_line(line)
}

/// Parses an already-decoded `serde_json::Value` into a Codex message.
pub fn parse_codex_value(value: &Value) -> Option<CodexMessage> {
    serde_json::from_value(value.clone()).ok()
}

/// Extracts a thread id from a Codex message if present.
pub fn extract_thread_id(msg: &CodexMessage) -> Option<String> {
    msg.params
        .as_ref()
        .and_then(|p| p.get(KEY_THREAD_ID).or_else(|| p.get(KEY_THREAD_ID_ALT)))
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            msg.result
                .as_ref()
                .and_then(|r| r.get("thread"))
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
}

/// Extracts the text field from a Codex delta object.
///
/// Tries the common shapes used by agent message and reasoning deltas:
/// `{"delta":{"text":"..."}}` and `{"delta":"..."}`.
fn extract_delta_text(params: &Value) -> Option<String> {
    let delta = params.get(KEY_DELTA)?;
    if let Some(text) = delta.get(KEY_TEXT).and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    delta.as_str().map(|s| s.to_string())
}

/// Converts a Codex app-server notification into a normalized `ProcessEvent`.
pub fn to_process_event(msg: &CodexMessage) -> Option<ProcessEvent> {
    let method = msg.method.as_deref()?;
    let params = msg.params.as_ref()?;

    match method {
        METHOD_ITEM_AGENT_MESSAGE_DELTA => {
            let text = extract_delta_text(params)?;
            Some(ProcessEvent::TextDelta { text })
        }
        METHOD_ITEM_REASONING_SUMMARY_TEXT_DELTA | METHOD_ITEM_REASONING_TEXT_DELTA => {
            // Codex streams reasoning summaries (`summaryTextDelta`) and raw reasoning
            // text (`textDelta`) as the model thinks. Treat both as thinking so the
            // frontend can fold them into the reasoning card instead of mixing them
            // with the final assistant output.
            let content = extract_delta_text(params).unwrap_or_default();
            if content.is_empty() {
                None
            } else {
                Some(ProcessEvent::Thinking { content })
            }
        }
        METHOD_ITEM_REASONING_SUMMARY_PART_ADDED => {
            // Boundary marker between reasoning summary sections; no user-visible text.
            None
        }
        METHOD_ITEM_STARTED => {
            let item = params.get(KEY_ITEM)?;
            let item_type = item
                .get(KEY_TYPE)
                .or_else(|| item.get(KEY_ITEM_TYPE))
                .and_then(|v| v.as_str())?;
            // item.id 在 item/started 与 item/completed 间保持稳定，
            // 透传为调用 id 供下游精确关联 ToolUse 与 ToolResult。
            let item_id = item
                .get(KEY_ID)
                .and_then(|v| v.as_str())
                .map(String::from);
            match item_type {
                ITEM_TYPE_COMMAND_EXECUTION | ITEM_TYPE_EXEC_COMMAND | ITEM_TYPE_SHELL => {
                    let command = item.get(KEY_COMMAND).and_then(|v| v.as_str()).unwrap_or("");
                    Some(ProcessEvent::ToolUse {
                        name: "shell".to_string(),
                        input: serde_json::json!({ "command": command }),
                        id: item_id,
                    })
                }
                ITEM_TYPE_MCP_TOOL_CALL | ITEM_TYPE_TOOL_CALL => {
                    let name = item
                        .get(KEY_NAME)
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let input = item.get(KEY_ARGUMENTS).cloned().unwrap_or(Value::Null);
                    Some(ProcessEvent::ToolUse {
                        name,
                        input,
                        id: item_id,
                    })
                }
                _ => None,
            }
        }
        METHOD_ITEM_COMPLETED => {
            let item = params.get(KEY_ITEM)?;
            let item_type = item
                .get(KEY_TYPE)
                .or_else(|| item.get(KEY_ITEM_TYPE))
                .and_then(|v| v.as_str())?;
            let item_id = item
                .get(KEY_ID)
                .and_then(|v| v.as_str())
                .map(String::from);
            match item_type {
                ITEM_TYPE_AGENT_MESSAGE => {
                    let text = item
                        .get(KEY_TEXT)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if text.is_empty() {
                        None
                    } else {
                        Some(ProcessEvent::TextDelta { text })
                    }
                }
                ITEM_TYPE_REASONING => {
                    // Completed reasoning item carries the accumulated summary/content.
                    // Prefer `summary` (readable) over `content` (raw) when both exist.
                    let content = item
                        .get(KEY_SUMMARY)
                        .and_then(|v| v.as_str())
                        .or_else(|| item.get(KEY_CONTENT).and_then(|v| v.as_str()))
                        .unwrap_or("")
                        .to_string();
                    if content.is_empty() {
                        None
                    } else {
                        Some(ProcessEvent::Thinking { content })
                    }
                }
                ITEM_TYPE_COMMAND_EXECUTION | ITEM_TYPE_EXEC_COMMAND | ITEM_TYPE_SHELL => {
                    let output = item.get(KEY_OUTPUT).and_then(|v| v.as_str()).unwrap_or("");
                    let exit_code = item
                        .get(KEY_EXIT_CODE)
                        .or_else(|| item.get(KEY_EXIT_CODE_ALT))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    Some(ProcessEvent::ToolResult {
                        name: "shell".to_string(),
                        result: output.to_string(),
                        failed: exit_code != 0,
                        id: item_id,
                    })
                }
                ITEM_TYPE_MCP_TOOL_CALL | ITEM_TYPE_TOOL_CALL => {
                    let name = item
                        .get(KEY_NAME)
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let result = item
                        .get(KEY_OUTPUT)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let failed = item
                        .get(KEY_FAILED)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    Some(ProcessEvent::ToolResult {
                        name,
                        result,
                        failed,
                        id: item_id,
                    })
                }
                _ => None,
            }
        }
        METHOD_TURN_COMPLETED => Some(ProcessEvent::Done),
        METHOD_TURN_FAILED => {
            let message = params
                .get(KEY_ERROR)
                .and_then(|e| e.get(KEY_MESSAGE))
                .or_else(|| params.get(KEY_MESSAGE))
                .and_then(|v| v.as_str())
                .unwrap_or("turn failed")
                .to_string();
            Some(ProcessEvent::Error { message })
        }
        METHOD_ERROR => {
            let message = params
                .get(KEY_MESSAGE)
                .and_then(|v| v.as_str())
                .unwrap_or("codex error")
                .to_string();
            Some(ProcessEvent::Error { message })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_delta() {
        let line = r#"{"method":"item/agentMessage/delta","params":{"threadId":"t1","delta":{"text":"hello"}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(matches!(ev, ProcessEvent::TextDelta { text } if text == "hello"));
    }

    #[test]
    fn test_parse_turn_completed() {
        let line = r#"{"method":"turn/completed","params":{"threadId":"t1","turn":{"status":"completed"}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(matches!(ev, ProcessEvent::Done));
    }

    #[test]
    fn test_parse_command_tool() {
        let line = r#"{"method":"item/started","params":{"item":{"id":"i1","type":"command_execution","command":"ls"}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::ToolUse { ref name, ref id, .. } if name == "shell" && id.as_deref() == Some("i1")),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_command_tool_result_carries_item_id() {
        // item/completed 携带与 item/started 相同的 item.id，必须透传到 ToolResult。
        let line = r#"{"method":"item/completed","params":{"item":{"id":"i1","type":"command_execution","output":"file list","exitCode":0}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::ToolResult { ref name, ref id, failed, .. }
                if name == "shell" && id.as_deref() == Some("i1") && !failed),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_mcp_tool_call_carries_item_id() {
        let line = r#"{"method":"item/started","params":{"item":{"id":"mcp-1","type":"mcp_tool_call","name":"read_file","arguments":{"path":"/tmp/a"}}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::ToolUse { ref name, ref id, .. }
                if name == "read_file" && id.as_deref() == Some("mcp-1")),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_reasoning_summary_text_delta() {
        let line = r#"{"method":"item/reasoning/summaryTextDelta","params":{"threadId":"t1","delta":{"text":"I need to check the file structure first."}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::Thinking { ref content } if content == "I need to check the file structure first."),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_reasoning_text_delta() {
        let line = r#"{"method":"item/reasoning/textDelta","params":{"threadId":"t1","delta":{"text":"Raw reasoning token"}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::Thinking { ref content } if content == "Raw reasoning token"),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_reasoning_summary_part_added_ignored() {
        let line = r#"{"method":"item/reasoning/summaryPartAdded","params":{"threadId":"t1","summaryIndex":1}}"#;
        let msg = parse_codex_line(line).unwrap();
        assert!(to_process_event(&msg).is_none());
    }

    #[test]
    fn test_parse_completed_reasoning_item() {
        let line = r#"{"method":"item/completed","params":{"threadId":"t1","item":{"id":"i1","type":"reasoning","summary":"Readable reasoning summary","content":"raw reasoning"}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::Thinking { ref content } if content == "Readable reasoning summary"),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_completed_reasoning_item_fallback_to_content() {
        let line = r#"{"method":"item/completed","params":{"threadId":"t1","item":{"id":"i1","type":"reasoning","content":"raw reasoning content"}}}"#;
        let msg = parse_codex_line(line).unwrap();
        let ev = to_process_event(&msg).unwrap();
        assert!(
            matches!(ev, ProcessEvent::Thinking { ref content } if content == "raw reasoning content"),
            "unexpected event: {:?}",
            ev
        );
    }

    #[test]
    fn test_parse_empty_reasoning_delta_skipped() {
        let line = r#"{"method":"item/reasoning/textDelta","params":{"threadId":"t1","delta":{"text":""}}}"#;
        let msg = parse_codex_line(line).unwrap();
        assert!(to_process_event(&msg).is_none());
    }

    #[test]
    fn test_extract_thread_id_from_result() {
        let value = serde_json::json!({
            "id": 1,
            "result": { "thread": { "id": "th-123" } }
        });
        let msg = parse_codex_value(&value).unwrap();
        assert_eq!(extract_thread_id(&msg), Some("th-123".to_string()));
    }
}
