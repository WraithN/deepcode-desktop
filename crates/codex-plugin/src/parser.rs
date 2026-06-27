use agent_core::process::event::ProcessEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        .and_then(|p| p.get("threadId").or_else(|| p.get("thread_id")))
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

/// Converts a Codex app-server notification into a normalized `ProcessEvent`.
pub fn to_process_event(msg: &CodexMessage) -> Option<ProcessEvent> {
    let method = msg.method.as_deref()?;
    let params = msg.params.as_ref()?;

    match method {
        "item/agentMessage/delta" => {
            let text = params
                .get("delta")
                .and_then(|d| d.get("text"))
                .and_then(|v| v.as_str())?;
            Some(ProcessEvent::TextDelta {
                text: text.to_string(),
            })
        }
        "item/started" => {
            let item = params.get("item")?;
            let item_type = item
                .get("type")
                .or_else(|| item.get("item_type"))
                .and_then(|v| v.as_str())?;
            match item_type {
                "command_execution" | "exec_command" | "shell" => {
                    let command = item.get("command").and_then(|v| v.as_str()).unwrap_or("");
                    Some(ProcessEvent::ToolUse {
                        name: "shell".to_string(),
                        input: serde_json::json!({ "command": command }),
                    })
                }
                "mcp_tool_call" | "tool_call" => {
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let input = item.get("arguments").cloned().unwrap_or(Value::Null);
                    Some(ProcessEvent::ToolUse { name, input })
                }
                _ => None,
            }
        }
        "item/completed" => {
            let item = params.get("item")?;
            let item_type = item
                .get("type")
                .or_else(|| item.get("item_type"))
                .and_then(|v| v.as_str())?;
            match item_type {
                "agent_message" => {
                    let text = item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if text.is_empty() {
                        None
                    } else {
                        Some(ProcessEvent::TextDelta { text })
                    }
                }
                "command_execution" | "exec_command" | "shell" => {
                    let output = item.get("output").and_then(|v| v.as_str()).unwrap_or("");
                    let exit_code = item
                        .get("exit_code")
                        .or_else(|| item.get("exitCode"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    Some(ProcessEvent::ToolResult {
                        name: "shell".to_string(),
                        result: output.to_string(),
                        failed: exit_code != 0,
                    })
                }
                "mcp_tool_call" | "tool_call" => {
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let result = item
                        .get("output")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let failed = item
                        .get("failed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    Some(ProcessEvent::ToolResult {
                        name,
                        result,
                        failed,
                    })
                }
                _ => None,
            }
        }
        "turn/completed" => Some(ProcessEvent::Done),
        "turn/failed" => {
            let message = params
                .get("error")
                .and_then(|e| e.get("message"))
                .or_else(|| params.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("turn failed")
                .to_string();
            Some(ProcessEvent::Error { message })
        }
        "error" => {
            let message = params
                .get("message")
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
            matches!(ev, ProcessEvent::ToolUse { ref name, .. } if name == "shell"),
            "unexpected event: {:?}",
            ev
        );
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
