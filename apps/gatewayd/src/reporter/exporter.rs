use reqwest::Client;
use serde_json::Value;

use super::config::ReporterConfig;

pub struct AuditLogExporter {
    client: Client,
    endpoint: String,
    api_key: Option<String>,
}

impl AuditLogExporter {
    pub fn new(config: &ReporterConfig) -> Self {
        let endpoint = config.endpoint.clone().unwrap_or_default();
        Self {
            client: Client::new(),
            endpoint,
            api_key: config.api_key.clone(),
        }
    }

    pub async fn export(&self, request_body: Value) -> Result<(), ExportError> {
        let mut req = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.map_err(ExportError::Network)?;
        let status = resp.status();

        if status.is_success() {
            Ok(())
        } else if status.is_client_error() {
            let body = resp.text().await.unwrap_or_default();
            Err(ExportError::ClientError(status.as_u16(), body))
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(ExportError::ServerError(status.as_u16(), body))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Client error {0}: {1}")]
    ClientError(u16, String),
    #[error("Server error {0}: {1}")]
    ServerError(u16, String),
}
