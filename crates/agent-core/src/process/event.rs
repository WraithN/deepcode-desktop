use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessEvent {
    Init {
        session_id: String,
    },
    UserMessage {
        content: String,
    },
    AssistantMessage {
        content: String,
    },
    TextDelta {
        text: String,
    },
    Thinking {
        content: String,
    },
    ToolUse {
        name: String,
        input: Value,
        /// 上游 agent 的稳定工具调用 id（opencode 的 callID、claude 的
        /// `toolu_...`、codex 的 item.id），用于 ToolResult 精确关联；
        /// 上游未提供时为 None，下游回退到 FIFO 匹配。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    ToolResult {
        name: String,
        result: String,
        failed: bool,
        /// 与对应 ToolUse 相同的调用 id，语义同上。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    Permission {
        tool_name: String,
        action: String,
    },
    Question {
        questions: Vec<QuestionItem>,
    },
    TodoWrite {
        todos: Vec<TodoItem>,
    },
    /// 一条 assistant 消息结束（如 claude 的 message_stop）。
    /// 一个回合内可能出现多次（工具调用循环），仅用于关闭当前文本消息，
    /// 不代表整个回合结束。
    MessageEnd,
    /// 整个回合结束（如 claude 的 result、opencode 的 session.idle、
    /// codex 的 turn/completed）。RUN_FINISHED 等终态事件必须以此为准。
    Done,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionItem {
    pub id: String,
    pub text: String,
    /// 问题附带的参考选项（opencode question 工具输入中的 options 数组），
    /// 供前端展示候选答案；旧格式问题没有选项时为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QuestionOption>>,
}

/// question 工具单个参考选项。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
        let ev = ProcessEvent::TextDelta {
            text: "hello".into(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("text_delta"));
        let decoded: ProcessEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, decoded);
    }

    #[test]
    fn test_serde_question() {
        let ev = ProcessEvent::Question {
            questions: vec![QuestionItem {
                id: "q1".into(),
                text: "ok?".into(),
                options: Some(vec![QuestionOption {
                    label: "确认".into(),
                    description: Some("继续执行".into()),
                }]),
            }],
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("question"));
        assert!(s.contains("options"));
        let decoded: ProcessEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, decoded);
    }

    #[test]
    fn test_serde_tool_use_id_compat() {
        // 带 id 的 ToolUse 应序列化出 id 字段并可完整往返。
        let ev = ProcessEvent::ToolUse {
            name: "bash".into(),
            input: serde_json::json!({ "command": "ls" }),
            id: Some("call-1".into()),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"id\":\"call-1\""));
        let decoded: ProcessEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, decoded);

        // 兼容旧格式：无 id 字段的反序列化结果应为 None，且序列化时不输出 id。
        let legacy = r#"{"type":"tool_use","name":"bash","input":{}}"#;
        let decoded: ProcessEvent = serde_json::from_str(legacy).unwrap();
        assert!(
            matches!(decoded, ProcessEvent::ToolUse { id: None, .. }),
            "旧格式缺少 id 字段时应反序列化为 None"
        );
        let s = serde_json::to_string(&decoded).unwrap();
        assert!(!s.contains("\"id\""), "id 为 None 时不应序列化该字段");

        // ToolResult 同样兼容。
        let legacy = r#"{"type":"tool_result","name":"bash","result":"ok","failed":false}"#;
        let decoded: ProcessEvent = serde_json::from_str(legacy).unwrap();
        assert!(
            matches!(decoded, ProcessEvent::ToolResult { id: None, .. }),
            "旧格式缺少 id 字段时应反序列化为 None"
        );
    }

    #[test]
    fn test_question_item_options_compat() {
        // 旧格式 {id, text} 无 options 字段时应反序列化为 None。
        let item: QuestionItem =
            serde_json::from_str(r#"{"id":"q1","text":"ok?"}"#).unwrap();
        assert_eq!(item.options, None);
        let s = serde_json::to_string(&item).unwrap();
        assert!(!s.contains("options"), "options 为 None 时不应序列化该字段");
    }
}
