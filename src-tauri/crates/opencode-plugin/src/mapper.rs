use agent_core::event::AgentEvent;
use crate::parser::OpencodeRawEvent;

pub fn map_to_agent_event(raw: OpencodeRawEvent) -> Option<AgentEvent> {
    match raw {
        OpencodeRawEvent::Thinking { content } => Some(AgentEvent::Thinking { content }),
        OpencodeRawEvent::TextDelta { content } => Some(AgentEvent::TextDelta { content }),
        OpencodeRawEvent::ToolUse { name, args, part } => {
            if let Some(part) = part {
                // opencode CLI 实际格式
                let tool_name = part.tool.unwrap_or_else(|| "unknown".to_string());
                let output = part.state.and_then(|s| s.output).unwrap_or_default();
                Some(AgentEvent::ToolResult {
                    tool_name,
                    result: output,
                    failed: false,
                })
            } else if let Some(name) = name {
                // 传统格式
                Some(AgentEvent::ToolUse {
                    tool_name: name,
                    args: args.unwrap_or_default(),
                })
            } else {
                None
            }
        }
        OpencodeRawEvent::ToolResult { name, result, failed } => Some(AgentEvent::ToolResult {
            tool_name: name,
            result,
            failed: failed.unwrap_or(false),
        }),
        OpencodeRawEvent::AskPermission { message, tool } => Some(AgentEvent::AskPermission {
            message,
            tool_name: tool,
        }),
        OpencodeRawEvent::AskUser { questions } => Some(AgentEvent::AskUser { questions }),
        OpencodeRawEvent::Error { message } => Some(AgentEvent::Error { message }),
        OpencodeRawEvent::Done => Some(AgentEvent::Done),
        // opencode CLI 实际格式映射
        OpencodeRawEvent::StepStart { .. } => {
            Some(AgentEvent::Thinking { content: String::new() })
        }
        OpencodeRawEvent::StepFinish { .. } => None,
        OpencodeRawEvent::Text { part } => {
            if let Some(text) = part.text {
                if text == "Done" {
                    Some(AgentEvent::Done)
                } else {
                    Some(AgentEvent::TextDelta { content: text })
                }
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_map_thinking() {
        let raw = OpencodeRawEvent::Thinking { content: "planning".into() };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::Thinking { content }) if content == "planning"));
    }

    #[test]
    fn test_map_tool_use_legacy() {
        let raw = OpencodeRawEvent::ToolUse {
            name: Some("edit".into()),
            args: Some(json!({"path": "/tmp/a.txt"})),
            part: None,
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::ToolUse { tool_name, .. }) if tool_name == "edit"));
    }

    #[test]
    fn test_map_tool_use_opencode() {
        let raw = OpencodeRawEvent::ToolUse {
            name: None,
            args: None,
            part: Some(crate::parser::OpencodePart {
                part_type: Some("tool".into()),
                tool: Some("write".into()),
                text: None,
                state: Some(crate::parser::OpencodeToolState {
                    status: Some("completed".into()),
                    input: None,
                    output: Some("Wrote file.".into()),
                }),
            }),
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::ToolResult { tool_name, result, .. }) if tool_name == "write" && result == "Wrote file."));
    }

    #[test]
    fn test_map_tool_result_failed_defaults_false() {
        let raw = OpencodeRawEvent::ToolResult {
            name: "edit".into(),
            result: "ok".into(),
            failed: None,
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::ToolResult { failed, .. }) if failed == false));
    }

    #[test]
    fn test_map_tool_result_failed_true() {
        let raw = OpencodeRawEvent::ToolResult {
            name: "edit".into(),
            result: "err".into(),
            failed: Some(true),
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::ToolResult { failed, .. }) if failed == true));
    }

    #[test]
    fn test_map_ask_permission() {
        let raw = OpencodeRawEvent::AskPermission {
            message: "allow?".into(),
            tool: "write".into(),
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::AskPermission { tool_name, .. }) if tool_name == "write"));
    }

    #[test]
    fn test_map_done() {
        let raw = OpencodeRawEvent::Done;
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::Done)));
    }

    #[test]
    fn test_map_step_start() {
        let raw = OpencodeRawEvent::StepStart {
            part: crate::parser::OpencodePart {
                part_type: Some("step-start".into()),
                tool: None,
                text: None,
                state: None,
            },
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::Thinking { .. })));
    }

    #[test]
    fn test_map_text() {
        let raw = OpencodeRawEvent::Text {
            part: crate::parser::OpencodePart {
                part_type: Some("text".into()),
                tool: None,
                text: Some("hello".into()),
                state: None,
            },
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::TextDelta { content }) if content == "hello"));
    }

    #[test]
    fn test_map_text_done() {
        let raw = OpencodeRawEvent::Text {
            part: crate::parser::OpencodePart {
                part_type: Some("text".into()),
                tool: None,
                text: Some("Done".into()),
                state: None,
            },
        };
        let mapped = map_to_agent_event(raw);
        assert!(matches!(mapped, Some(AgentEvent::Done)));
    }
}
