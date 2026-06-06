use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpencodeRawEvent {
    // 传统格式（保留兼容）
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        args: Option<Value>,
        // opencode CLI 实际输出格式
        #[serde(default)]
        part: Option<OpencodePart>,
    },
    ToolResult { name: String, result: String, failed: Option<bool> },
    AskPermission { message: String, tool: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
    // opencode CLI 实际输出格式
    StepStart { part: OpencodePart },
    StepFinish { part: OpencodePart },
    Text { part: OpencodePart },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpencodePart {
    #[serde(rename = "type")]
    pub part_type: Option<String>,
    pub tool: Option<String>,
    pub text: Option<String>,
    pub state: Option<OpencodeToolState>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpencodeToolState {
    pub status: Option<String>,
    pub input: Option<Value>,
    pub output: Option<String>,
}

pub fn parse_opencode_json_line(line: &str) -> Option<OpencodeRawEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<OpencodeRawEvent>(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_empty() {
        assert!(parse_opencode_json_line("").is_none());
        assert!(parse_opencode_json_line("   ").is_none());
    }

    #[test]
    fn test_parse_thinking() {
        let line = r#"{"type":"thinking","content":"hello"}"#;
        let ev = parse_opencode_json_line(line).unwrap();
        match ev {
            OpencodeRawEvent::Thinking { content } => assert_eq!(content, "hello"),
            _ => panic!("expected Thinking"),
        }
    }

    #[test]
    fn test_parse_tool_use_legacy() {
        let line = r#"{"type":"tool_use","name":"read_file","args":{"path":"/tmp/a.txt"}}"#;
        let ev = parse_opencode_json_line(line).unwrap();
        match ev {
            OpencodeRawEvent::ToolUse { name, args, part } => {
                assert_eq!(name.as_deref(), Some("read_file"));
                assert_eq!(args, Some(json!({"path": "/tmp/a.txt"})));
                assert!(part.is_none());
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn test_parse_tool_use_opencode() {
        let line = r#"{"type":"tool_use","part":{"type":"tool","tool":"write","state":{"status":"completed","output":"Wrote file."}}}"#;
        let ev = parse_opencode_json_line(line).unwrap();
        match ev {
            OpencodeRawEvent::ToolUse { name, args, part } => {
                assert!(name.is_none());
                assert!(args.is_none());
                let p = part.unwrap();
                assert_eq!(p.tool.as_deref(), Some("write"));
                assert_eq!(p.state.unwrap().output.as_deref(), Some("Wrote file."));
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn test_parse_tool_result() {
        let line = r#"{"type":"tool_result","name":"read_file","result":"ok","failed":true}"#;
        let ev = parse_opencode_json_line(line).unwrap();
        match ev {
            OpencodeRawEvent::ToolResult { name, result, failed } => {
                assert_eq!(name, "read_file");
                assert_eq!(result, "ok");
                assert_eq!(failed, Some(true));
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn test_parse_done() {
        let line = r#"{"type":"done"}"#;
        let ev = parse_opencode_json_line(line).unwrap();
        assert!(matches!(ev, OpencodeRawEvent::Done));
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_opencode_json_line("not json").is_none());
        assert!(parse_opencode_json_line("{}").is_none()); // missing type field
    }
}
