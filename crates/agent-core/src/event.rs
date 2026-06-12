use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse { tool_name: String, args: Value },
    ToolResult { tool_name: String, result: String, failed: bool },
    AskPermission { message: String, tool_name: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_serde_thinking() {
        let ev = AgentEvent::Thinking { content: "hello".into() };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"type":"thinking","content":"hello"}"#);
    }

    #[test]
    fn test_event_serde_tool_use() {
        let ev = AgentEvent::ToolUse {
            tool_name: "read_file".into(),
            args: json!({"path": "/tmp/a.txt"}),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"type":"tool_use","tool_name":"read_file","args":{"path":"/tmp/a.txt"}}"#);
    }

    #[test]
    fn test_event_serde_done() {
        let ev = AgentEvent::Done;
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"type":"done"}"#);
    }
}
