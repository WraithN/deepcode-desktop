use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessEvent {
    Init { session_id: String },
    UserMessage { content: String },
    AssistantMessage { content: String },
    TextDelta { text: String },
    Thinking { content: String },
    ToolUse { name: String, input: Value },
    ToolResult { name: String, result: String, failed: bool },
    Permission { tool_name: String, action: String },
    Question { questions: Vec<QuestionItem> },
    TodoWrite { todos: Vec<TodoItem> },
    Done,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionItem {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub completed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_text_delta() {
        let ev = ProcessEvent::TextDelta { text: "hello".into() };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("text_delta"));
        let decoded: ProcessEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, decoded);
    }

    #[test]
    fn test_serde_question() {
        let ev = ProcessEvent::Question {
            questions: vec![QuestionItem { id: "q1".into(), text: "ok?".into() }],
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("question"));
    }
}
