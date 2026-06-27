use serde::{Deserialize, Serialize};
use serde_json::Value;

/// AG-UI message role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Developer,
    System,
    Assistant,
    User,
    Tool,
}

/// AG-UI message, tagged by role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    Developer {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    System {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Assistant {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(rename = "toolCalls", skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Value>,
    },
    User {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Tool {
        id: String,
        content: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            name: None,
        }
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            Message::Developer { content, .. }
            | Message::System { content, .. }
            | Message::User { content, .. }
            | Message::Tool { content, .. } => Some(content),
            Message::Assistant { content, .. } => content.as_deref(),
        }
    }

    pub fn role(&self) -> Role {
        match self {
            Message::Developer { .. } => Role::Developer,
            Message::System { .. } => Role::System,
            Message::Assistant { .. } => Role::Assistant,
            Message::User { .. } => Role::User,
            Message::Tool { .. } => Role::Tool,
        }
    }
}

/// Tool definition carried in RunAgentInput.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

/// Context item carried in RunAgentInput.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextItem {
    pub name: String,
    pub value: Value,
}

/// Input for starting an AG-UI agent run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunAgentInput {
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default = "empty_object")]
    pub state: Value,
    pub messages: Vec<Message>,
    #[serde(default = "Vec::new")]
    pub tools: Vec<Tool>,
    #[serde(default = "Vec::new")]
    pub context: Vec<ContextItem>,
    #[serde(rename = "forwardedProps", default = "empty_object")]
    pub forwarded_props: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Common fields present on every AG-UI event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BaseEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<Value>,
}

/// AG-UI event, tagged by type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Event {
    RunStarted {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
    },
    RunFinished {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
    },
    RunError {
        #[serde(flatten)]
        base: BaseEvent,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    TextMessageStart {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "messageId")]
        message_id: String,
        role: String,
    },
    TextMessageContent {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    TextMessageEnd {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "messageId")]
        message_id: String,
    },
    ThinkingTextMessageStart {
        #[serde(flatten)]
        base: BaseEvent,
    },
    ThinkingTextMessageContent {
        #[serde(flatten)]
        base: BaseEvent,
        delta: String,
    },
    ThinkingTextMessageEnd {
        #[serde(flatten)]
        base: BaseEvent,
    },
    ToolCallStart {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolCallName")]
        tool_call_name: String,
    },
    ToolCallArgs {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        delta: String,
    },
    ToolCallEnd {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
    },
    ToolCallResult {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        content: String,
    },
    StateSnapshot {
        #[serde(flatten)]
        base: BaseEvent,
        snapshot: Value,
    },
    StateDelta {
        #[serde(flatten)]
        base: BaseEvent,
        delta: Vec<Value>,
    },
    MessagesSnapshot {
        #[serde(flatten)]
        base: BaseEvent,
        messages: Vec<Message>,
    },
    Raw {
        #[serde(flatten)]
        base: BaseEvent,
        event: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    Custom {
        #[serde(flatten)]
        base: BaseEvent,
        name: String,
        value: Value,
    },
    StepStarted {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "stepName")]
        step_name: String,
    },
    StepFinished {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "stepName")]
        step_name: String,
    },
}
