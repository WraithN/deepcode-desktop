use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent_type: String,
    pub model: String,
    pub workspace: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Idle,
    Closed,
}

impl Session {
    pub fn new(agent_type: String, model: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            agent_type,
            model,
            workspace: None,
            started_at: now,
            last_active_at: now,
            status: SessionStatus::Active,
        }
    }
}
