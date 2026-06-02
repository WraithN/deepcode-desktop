use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpencodeRawEvent {
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse { name: String, args: Value },
    ToolResult { name: String, result: String, failed: Option<bool> },
    AskPermission { message: String, tool: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
}

pub fn parse_opencode_json_line(line: &str) -> Option<OpencodeRawEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<OpencodeRawEvent>(trimmed).ok()
}
