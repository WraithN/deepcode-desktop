use agent_core::event::AgentEvent;
use serde_json::Value;

pub fn parse_notification_to_event(params: &Value) -> Option<AgentEvent> {
    let _conversation_id = params.get("conversation_id").and_then(|v| v.as_str())?;
    let delta = params.get("delta")?;
    let delta_type = delta.get("type").and_then(|v| v.as_str())?;

    match delta_type {
        "thinking" => {
            let content = delta.get("content").and_then(|v| v.as_str())?;
            Some(AgentEvent::Thinking {
                content: content.to_string(),
            })
        }
        "text_delta" => {
            let content = delta.get("content").and_then(|v| v.as_str())?;
            Some(AgentEvent::TextDelta {
                content: content.to_string(),
            })
        }
        "tool_use" => {
            let tool_name = delta.get("tool_name").and_then(|v| v.as_str())?;
            let args = delta.get("args").cloned().unwrap_or(Value::Null);
            Some(AgentEvent::ToolUse {
                tool_name: tool_name.to_string(),
                args,
            })
        }
        "tool_result" => {
            let tool_name = delta.get("tool_name").and_then(|v| v.as_str())?;
            let result = delta.get("result").and_then(|v| v.as_str())?;
            let failed = delta
                .get("failed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(AgentEvent::ToolResult {
                tool_name: tool_name.to_string(),
                result: result.to_string(),
                failed,
            })
        }
        "done" => Some(AgentEvent::Done),
        "error" => {
            let message = delta.get("message").and_then(|v| v.as_str())?;
            Some(AgentEvent::Error {
                message: message.to_string(),
            })
        }
        _ => None,
    }
}
