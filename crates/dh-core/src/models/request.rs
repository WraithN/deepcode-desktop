use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedRequest {
    pub id: String,
    pub session_id: String,
    pub provider: Provider,
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub metadata: RequestMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestMetadata {
    pub agent_type: Option<String>,
    pub workspace: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl UnifiedRequest {
    pub fn new(provider: Provider, model: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4().to_string(),
            provider,
            model,
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            stream: true,
            metadata: RequestMetadata {
                timestamp: chrono::Utc::now(),
                ..Default::default()
            },
        }
    }

    pub fn prepend_system_message(&mut self, content: String) {
        self.messages.insert(0, Message {
            role: Role::System,
            content,
        });
    }
}
