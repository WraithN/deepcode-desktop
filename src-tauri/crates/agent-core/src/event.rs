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
