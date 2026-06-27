use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Bidirectional mapping between conversation ids and agent session ids.
///
/// Many agent runtimes identify sessions independently from the frontend
/// conversation id. This structure keeps both directions in sync so that
/// incoming agent events can be routed back to the correct conversation.
#[derive(Clone, Default)]
pub struct ConversationSessionMap {
    conversation_to_session: Arc<Mutex<HashMap<String, String>>>,
    session_to_conversation: Arc<Mutex<HashMap<String, String>>>,
}

impl ConversationSessionMap {
    /// Creates an empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a bidirectional mapping between `conversation_id` and `session_id`.
    pub fn insert(&self, conversation_id: &str, session_id: &str) {
        let mut c2s = self.conversation_to_session.lock().unwrap();
        let mut s2c = self.session_to_conversation.lock().unwrap();
        c2s.insert(conversation_id.to_string(), session_id.to_string());
        s2c.insert(session_id.to_string(), conversation_id.to_string());
    }

    /// Returns the session id associated with `conversation_id`, if any.
    pub fn session_for_conversation(&self, conversation_id: &str) -> Option<String> {
        self.conversation_to_session
            .lock()
            .unwrap()
            .get(conversation_id)
            .cloned()
    }

    /// Returns the conversation id associated with `session_id`, if any.
    pub fn conversation_for_session(&self, session_id: &str) -> Option<String> {
        self.session_to_conversation
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
    }

    /// Clears all mappings.
    pub fn clear(&self) {
        self.conversation_to_session.lock().unwrap().clear();
        self.session_to_conversation.lock().unwrap().clear();
    }

    /// Looks up the session for `conversation_id`.
    ///
    /// If no mapping exists and `fallback_session` is provided, the fallback is
    /// stored as the session for this conversation (bidirectionally) and
    /// returned. This is useful for long-running agent processes that maintain a
    /// single active session.
    pub fn resolve_or_fallback(
        &self,
        conversation_id: &str,
        fallback_session: Option<&str>,
    ) -> Option<String> {
        if let Some(session_id) = self.session_for_conversation(conversation_id) {
            return Some(session_id);
        }

        let session_id = fallback_session?;
        self.insert(conversation_id, session_id);
        Some(session_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_lookup() {
        let map = ConversationSessionMap::new();
        map.insert("c-1", "s-1");
        assert_eq!(map.session_for_conversation("c-1").as_deref(), Some("s-1"));
        assert_eq!(map.conversation_for_session("s-1").as_deref(), Some("c-1"));
    }

    #[test]
    fn test_resolve_with_fallback() {
        let map = ConversationSessionMap::new();
        assert_eq!(
            map.resolve_or_fallback("c-1", Some("s-1")).as_deref(),
            Some("s-1")
        );
        assert_eq!(map.session_for_conversation("c-1").as_deref(), Some("s-1"));
    }

    #[test]
    fn test_resolve_without_fallback_returns_none() {
        let map = ConversationSessionMap::new();
        assert!(map.resolve_or_fallback("c-1", None).is_none());
    }
}
