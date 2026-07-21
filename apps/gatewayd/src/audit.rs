use dh_core::{AuditLogEntry, Direction, estimate_tokens};
use dh_db::DbManager;
use rusqlite::params;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

const AUDIT_QUEUE_CAPACITY: usize = 8192;

pub struct AuditLogger {
    sender: mpsc::Sender<AuditLogEntry>,
}

impl AuditLogger {
    pub fn new() -> (Self, mpsc::Receiver<AuditLogEntry>) {
        let (sender, receiver) = mpsc::channel(AUDIT_QUEUE_CAPACITY);
        (Self { sender }, receiver)
    }

    pub fn log(&self, entry: AuditLogEntry) {
        match self.sender.try_send(entry) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                warn!("Audit log queue is full, dropping entry");
            }
            Err(e) => {
                error!("Failed to send audit log: {}", e);
            }
        }
    }
}

pub struct AuditStorage {
    db_path: std::path::PathBuf,
}

impl AuditStorage {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self { db_path }
    }

    async fn insert(&self, entry: AuditLogEntry) -> anyhow::Result<()> {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut db = DbManager::open(&db_path)?;
            let conn = db.conn_mut();
            conn.execute(
                r#"
                INSERT INTO audit_logs (
                    id, session_id, request_id, direction, provider, model,
                    agent_type, payload, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens,
                    timestamp, metadata
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#,
                params![
                    &entry.id,
                    &entry.session_id,
                    &entry.request_id,
                    format!("{:?}", entry.direction).to_lowercase(),
                    &entry.provider,
                    &entry.model,
                    entry.agent_type.as_deref(),
                    entry.payload.as_deref(),
                    entry.payload_size_bytes as i64,
                    entry.token_usage.as_ref().map(|u| u.prompt_tokens as i64),
                    entry.token_usage.as_ref().map(|u| u.completion_tokens as i64),
                    entry.token_usage.as_ref().map(|u| u.total_tokens as i64),
                    entry.timestamp.to_rfc3339(),
                    entry.metadata.to_string(),
                ],
            )?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("audit insert task failed: {}", e))??;
        Ok(())
    }
}

/// 从 JSON 响应体中提取 usage
pub fn extract_usage_from_json(body: &[u8]) -> Option<dh_core::TokenUsage> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let usage = json.get("usage")?;
    Some(dh_core::TokenUsage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage.get("completion_tokens")?.as_u64()? as u32,
        total_tokens: usage.get("total_tokens")?.as_u64()? as u32,
    })
}

/// 从 SSE 流文本中提取最后一个 usage chunk
pub fn extract_usage_from_sse(text: &str) -> Option<dh_core::TokenUsage> {
    let mut last_usage: Option<dh_core::TokenUsage> = None;
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Some(usage) = extract_usage_from_json(data.as_bytes()) {
                last_usage = Some(usage);
            }
        }
    }
    last_usage
}

/// 创建 Response 审计日志并发送
pub fn log_response_audit(
    audit: &AuditLogger,
    session_id: String,
    request_id: String,
    provider: String,
    model: String,
    bytes: &[u8],
    request_body: &str,
) {
    let usage = extract_usage_from_json(bytes).or_else(|| {
        let text = String::from_utf8_lossy(bytes);
        extract_usage_from_sse(&text)
    });

    let usage = match usage {
        Some(u) => u,
        None => {
            let response_text = String::from_utf8_lossy(bytes);
            dh_core::TokenUsage {
                prompt_tokens: estimate_tokens(request_body, &model),
                completion_tokens: estimate_tokens(&response_text, &model),
                total_tokens: 0,
            }
        }
    };

    let mut entry =
        AuditLogEntry::new(session_id, request_id, Direction::Response, provider, model);
    entry.token_usage = Some(usage);
    entry.payload_size_bytes = bytes.len();
    entry.metadata = serde_json::json!({
        "token_source": if extract_usage_from_json(bytes).is_some() || extract_usage_from_sse(&String::from_utf8_lossy(bytes)).is_some() {
            "provider"
        } else {
            "estimated"
        }
    });
    audit.log(entry);
}

pub async fn run_storage_worker(
    mut receiver: mpsc::Receiver<AuditLogEntry>,
    storage: AuditStorage,
) {
    info!("Audit storage worker started");
    while let Some(entry) = receiver.recv().await {
        if let Err(e) = storage.insert(entry).await {
            error!("Failed to persist audit log: {}", e);
        }
    }
    info!("Audit storage worker stopped");
}

