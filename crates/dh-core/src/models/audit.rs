use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub session_id: String,
    pub request_id: String,
    pub direction: Direction,
    pub provider: String,
    pub model: String,
    pub agent_type: Option<String>,
    pub payload: Option<String>,
    pub payload_size_bytes: usize,
    pub token_usage: Option<crate::TokenUsage>,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Request,
    Response,
}

impl AuditLogEntry {
    pub fn new(
        session_id: String,
        request_id: String,
        direction: Direction,
        provider: String,
        model: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            request_id,
            direction,
            provider,
            model,
            agent_type: None,
            payload: None,
            payload_size_bytes: 0,
            token_usage: None,
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }
}
