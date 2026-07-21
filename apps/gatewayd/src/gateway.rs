use axum::body::Body;
use axum::response::Response;
use dh_core::{Message, Provider, Role, UnifiedRequest};
use reqwest::Client;
use serde_json::Value;
use tracing::info;

pub struct GatewayRouter {
    client: Client,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
    deepseek_api_key: Option<String>,
}

impl Default for GatewayRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayRouter {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
        }
    }

    pub async fn forward_openai(
        &self,
        provider: &str,
        body: String,
    ) -> Result<Response, anyhow::Error> {
        let (url, api_key) = match provider {
            "deepseek" => {
                let key = self
                    .deepseek_api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("DEEPSEEK_API_KEY not set"))?;
                ("https://api.deepseek.com/v1/chat/completions", key)
            }
            _ => {
                let key = self
                    .openai_api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
                ("https://api.openai.com/v1/chat/completions", key)
            }
        };

        info!("Forwarding {} request to {}", provider, url);

        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();

        let stream = resp.bytes_stream();
        let body = Body::from_stream(stream);

        let mut builder = Response::builder().status(status);
        builder = builder.header("Content-Type", content_type);
        let response = builder.body(body)?;

        Ok(response)
    }

    pub async fn forward_anthropic(&self, body: String) -> Result<Response, anyhow::Error> {
        let api_key = self
            .anthropic_api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        info!("Forwarding request to Anthropic API");

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();

        let stream = resp.bytes_stream();
        let body = Body::from_stream(stream);

        let mut builder = Response::builder().status(status);
        builder = builder.header("Content-Type", content_type);
        let response = builder.body(body)?;

        Ok(response)
    }
}

pub fn openai_to_unified(body: Value) -> UnifiedRequest {
    let mut req = UnifiedRequest::new(
        Provider::OpenAi,
        body.get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("gpt-4o")
            .to_string(),
    );

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        req.messages = messages
            .iter()
            .filter_map(|m| {
                let role = m.get("role")?.as_str()?;
                let content = m.get("content")?.as_str()?;
                let role = match role {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Some(Message {
                    role,
                    content: content.to_string(),
                })
            })
            .collect();
    }

    req.temperature = body
        .get("temperature")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    req.max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    req.stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(true);

    req
}

pub fn anthropic_to_unified(body: Value) -> UnifiedRequest {
    let mut req = UnifiedRequest::new(
        Provider::Anthropic,
        body.get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4")
            .to_string(),
    );

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        req.messages = messages
            .iter()
            .filter_map(|m| {
                let role = m.get("role")?.as_str()?;
                let content = m.get("content")?.as_str()?;
                let role = match role {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Some(Message {
                    role,
                    content: content.to_string(),
                })
            })
            .collect();
    }

    if let Some(system) = body.get("system").and_then(|v| v.as_str()) {
        req.prepend_system_message(system.to_string());
    }

    req.max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    req.temperature = body
        .get("temperature")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);

    req
}

pub fn unified_to_openai_json(req: &UnifiedRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                },
                "content": m.content
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "stream": req.stream,
    });

    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }

    body
}

pub fn unified_to_anthropic_json(req: &UnifiedRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .filter_map(|m| match m.role {
            Role::System => None,
            _ => Some(serde_json::json!({
                "role": match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    _ => "user",
                },
                "content": m.content
            })),
        })
        .collect();

    let system_prompt = req
        .messages
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
    });

    if !system_prompt.is_empty() {
        body["system"] = serde_json::json!(system_prompt);
    }
    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    body
}

pub fn resolve_provider(model: &str) -> &'static str {
    if model.starts_with("deepseek") {
        "deepseek"
    } else if model.starts_with("gpt") || model.starts_with("text-") {
        "openai"
    } else if model.starts_with("claude") {
        "anthropic"
    } else {
        "unknown"
    }
}
