use crate::event_sink::DynEventSink;
use crate::process::event::ProcessEvent;
use serde_json::{json, Map, Value};

const METHOD_TOKEN: &str = "agent.token";
const METHOD_THINKING: &str = "agent.thinking";
const METHOD_PERMISSION: &str = "agent.permission";
const METHOD_QUESTION: &str = "agent.question";
const METHOD_TODO_WRITE: &str = "agent.todowrite";
const METHOD_DONE: &str = "agent.done";
const METHOD_ERROR: &str = "agent.error";

const INTERACTION_TYPE_PERMISSION: &str = "permission";
const INTERACTION_TYPE_QUESTION: &str = "question";
const INTERACTION_TYPE_TODO_WRITE: &str = "todowrite";

const THINKING_TYPE_STEP_START: &str = "step-start";
const THINKING_ID_PREFIX: &str = "thinking-";

const KEY_INSTANCE_ID: &str = "instance_id";
const KEY_CONVERSATION_ID: &str = "conversation_id";
const KEY_SESSION_ID: &str = "sessionID";
const KEY_INTERACTION: &str = "interaction";
const KEY_TYPE: &str = "type";
const KEY_TEXT: &str = "text";
const KEY_CONTENT: &str = "content";
const KEY_ID: &str = "id";
const KEY_TOOL_NAME: &str = "toolName";
const KEY_ACTION: &str = "action";
const KEY_QUESTIONS: &str = "questions";
const KEY_TODOS: &str = "todos";
const KEY_MESSAGE: &str = "message";

#[cfg(test)]
const KEY_COMPLETED: &str = "completed";

/// Maps raw [`ProcessEvent`]s from an agent process into frontend-facing
/// JSON-RPC payloads and forwards them through an [`EventSink`].
pub struct EventMapper {
    instance_id: String,
    conversation_id: String,
}

impl EventMapper {
    /// Creates a new mapper for the given instance and conversation.
    pub fn new(instance_id: String, conversation_id: String) -> Self {
        Self {
            instance_id,
            conversation_id,
        }
    }

    /// Builds the common base payload containing routing identifiers.
    fn base_payload(&self) -> Value {
        json!({
            KEY_INSTANCE_ID: self.instance_id,
            KEY_CONVERSATION_ID: self.conversation_id,
        })
    }

    /// Emits a JSON-RPC notification with the base payload merged with `extra`.
    ///
    /// `extra` must be a JSON object; if it is not, only the base payload is emitted.
    fn emit_with_base(&self, sink: &DynEventSink, method: &str, extra: Value) {
        let mut payload = self.base_payload();
        if let Some(obj) = payload.as_object_mut() {
            if let Value::Object(extra_obj) = extra {
                obj.extend(extra_obj);
            }
        }
        sink.emit(method, payload);
    }

    /// Emits an interaction-style notification used by permission/question/todo events.
    ///
    /// `interaction` must be a JSON object; the caller is expected to pass a properly
    /// shaped interaction value.
    fn emit_interaction(
        &self,
        sink: &DynEventSink,
        method: &str,
        interaction_type: &str,
        interaction: Value,
    ) {
        let mut payload = self.base_payload();
        let Some(obj) = payload.as_object_mut() else {
            sink.emit(method, payload);
            return;
        };

        obj.insert(
            KEY_SESSION_ID.to_string(),
            self.conversation_id.clone().into(),
        );

        let mut interaction_obj = Map::new();
        interaction_obj.insert(KEY_TYPE.to_string(), interaction_type.into());
        if let Value::Object(extra) = interaction {
            interaction_obj.extend(extra);
        }
        obj.insert(
            KEY_INTERACTION.to_string(),
            Value::Object(interaction_obj),
        );

        sink.emit(method, payload);
    }

    /// Converts `event` into the appropriate JSON-RPC notification and emits it.
    ///
    /// `Init`, `UserMessage`, `AssistantMessage`, `ToolUse`, and `ToolResult` are
    /// intentionally not mapped to frontend events because they are handled by other
    /// consumers or are internal. Empty `TextDelta` values are skipped to avoid noise.
    pub fn map(&self, event: ProcessEvent, sink: &DynEventSink) {
        match event {
            ProcessEvent::TextDelta { text } if !text.is_empty() => {
                self.emit_with_base(sink, METHOD_TOKEN, json!({ KEY_TEXT: text }));
            }
            ProcessEvent::TextDelta { .. } => {}
            ProcessEvent::Thinking { content } => {
                self.emit_with_base(
                    sink,
                    METHOD_THINKING,
                    json!({
                        KEY_CONTENT: content,
                        KEY_ID: format!("{THINKING_ID_PREFIX}{}", self.instance_id),
                        KEY_TYPE: THINKING_TYPE_STEP_START,
                    }),
                );
            }
            ProcessEvent::Permission { tool_name, action } => {
                self.emit_interaction(
                    sink,
                    METHOD_PERMISSION,
                    INTERACTION_TYPE_PERMISSION,
                    json!({
                        KEY_TOOL_NAME: tool_name,
                        KEY_ACTION: action,
                    }),
                );
            }
            ProcessEvent::Question { questions } => {
                self.emit_interaction(
                    sink,
                    METHOD_QUESTION,
                    INTERACTION_TYPE_QUESTION,
                    json!({ KEY_QUESTIONS: questions }),
                );
            }
            ProcessEvent::TodoWrite { todos } => {
                self.emit_interaction(
                    sink,
                    METHOD_TODO_WRITE,
                    INTERACTION_TYPE_TODO_WRITE,
                    json!({ KEY_TODOS: todos }),
                );
            }
            ProcessEvent::Done => {
                sink.emit(METHOD_DONE, self.base_payload());
            }
            ProcessEvent::Error { message } => {
                self.emit_with_base(sink, METHOD_ERROR, json!({ KEY_MESSAGE: message }));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_sink::{DynEventSink, EventSink};
    use crate::process::event::{ProcessEvent, QuestionItem, TodoItem};
    use std::sync::{Arc, Mutex};

    const TEST_INSTANCE_ID: &str = "i-1";
    const TEST_CONVERSATION_ID: &str = "c-1";

    #[derive(Clone, Default)]
    struct MockSink {
        events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl EventSink for MockSink {
        fn emit(&self, event_type: &str, payload: serde_json::Value) {
            self.events
                .lock()
                .unwrap()
                .push((event_type.to_string(), payload));
        }
    }

    fn mapper() -> EventMapper {
        EventMapper::new(TEST_INSTANCE_ID.into(), TEST_CONVERSATION_ID.into())
    }

    fn wrap(sink: MockSink) -> DynEventSink {
        Arc::new(sink)
    }

    fn map_event(event: ProcessEvent) -> Vec<(String, serde_json::Value)> {
        let sink = MockSink::default();
        mapper().map(event, &wrap(sink.clone()));
        let guard = sink.events.lock().unwrap();
        guard.clone()
    }

    #[test]
    fn test_map_text_delta() {
        let events = map_event(ProcessEvent::TextDelta { text: "hi".into() });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_TOKEN);
        assert_eq!(events[0].1[KEY_TEXT], "hi");
        assert_eq!(events[0].1[KEY_INSTANCE_ID], TEST_INSTANCE_ID);
        assert_eq!(events[0].1[KEY_CONVERSATION_ID], TEST_CONVERSATION_ID);
    }

    #[test]
    fn test_map_empty_text_delta_skipped() {
        let events = map_event(ProcessEvent::TextDelta { text: "".into() });
        assert!(events.is_empty());
    }

    #[test]
    fn test_map_thinking() {
        let events = map_event(ProcessEvent::Thinking { content: "...".into() });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_THINKING);
        assert_eq!(events[0].1[KEY_CONTENT], "...");
        assert_eq!(events[0].1[KEY_ID], "thinking-i-1");
        assert_eq!(events[0].1[KEY_TYPE], THINKING_TYPE_STEP_START);
    }

    #[test]
    fn test_map_permission() {
        let events = map_event(ProcessEvent::Permission {
            tool_name: "bash".into(),
            action: "run".into(),
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_PERMISSION);
        assert_eq!(events[0].1[KEY_SESSION_ID], TEST_CONVERSATION_ID);
        assert_eq!(events[0].1[KEY_INTERACTION][KEY_TOOL_NAME], "bash");
        assert_eq!(
            events[0].1[KEY_INTERACTION][KEY_TYPE],
            INTERACTION_TYPE_PERMISSION
        );
    }

    #[test]
    fn test_map_question() {
        let events = map_event(ProcessEvent::Question {
            questions: vec![QuestionItem {
                id: "q1".into(),
                text: "ok?".into(),
            }],
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_QUESTION);
        assert_eq!(events[0].1[KEY_INTERACTION][KEY_QUESTIONS][0][KEY_TEXT], "ok?");
        assert_eq!(
            events[0].1[KEY_INTERACTION][KEY_TYPE],
            INTERACTION_TYPE_QUESTION
        );
    }

    #[test]
    fn test_map_todo_write() {
        let events = map_event(ProcessEvent::TodoWrite {
            todos: vec![TodoItem {
                id: "t1".into(),
                text: "do it".into(),
                completed: false,
            }],
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_TODO_WRITE);
        assert!(!events[0].1[KEY_INTERACTION][KEY_TODOS][0][KEY_COMPLETED]
            .as_bool()
            .unwrap());
        assert_eq!(
            events[0].1[KEY_INTERACTION][KEY_TYPE],
            INTERACTION_TYPE_TODO_WRITE
        );
    }

    #[test]
    fn test_map_done() {
        let events = map_event(ProcessEvent::Done);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_DONE);
    }

    #[test]
    fn test_map_error() {
        let events = map_event(ProcessEvent::Error {
            message: "boom".into(),
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, METHOD_ERROR);
        assert_eq!(events[0].1[KEY_MESSAGE], "boom");
    }
}
