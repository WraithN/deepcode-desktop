use agent_core::event::AgentEvent;
use crate::parser::OpencodeRawEvent;

pub fn map_to_agent_event(raw: OpencodeRawEvent) -> AgentEvent {
    match raw {
        OpencodeRawEvent::Thinking { content } => AgentEvent::Thinking { content },
        OpencodeRawEvent::TextDelta { content } => AgentEvent::TextDelta { content },
        OpencodeRawEvent::ToolUse { name, args } => AgentEvent::ToolUse {
            tool_name: name,
            args,
        },
        OpencodeRawEvent::ToolResult { name, result, failed } => AgentEvent::ToolResult {
            tool_name: name,
            result,
            failed: failed.unwrap_or(false),
        },
        OpencodeRawEvent::AskPermission { message, tool } => AgentEvent::AskPermission {
            message,
            tool_name: tool,
        },
        OpencodeRawEvent::AskUser { questions } => AgentEvent::AskUser { questions },
        OpencodeRawEvent::Error { message } => AgentEvent::Error { message },
        OpencodeRawEvent::Done => AgentEvent::Done,
    }
}
