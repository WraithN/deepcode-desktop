use dh_db::AuditLogRow;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// 将 AuditLogRow 转换为 OTLP JSON LogRecord
pub fn transform_audit_log(row: &AuditLogRow, sanitize: bool) -> Value {
    let mut attributes = Vec::new();

    attributes.push(json!({"key": "audit.log_id", "value": {"stringValue": row.id}}));
    attributes.push(json!({"key": "audit.request_id", "value": {"stringValue": row.request_id}}));
    attributes.push(json!({"key": "session.id", "value": {"stringValue": row.session_id}}));
    attributes.push(json!({"key": "llm.model", "value": {"stringValue": row.model}}));
    attributes.push(json!({"key": "llm.provider", "value": {"stringValue": row.provider}}));
    attributes.push(json!({"key": "llm.direction", "value": {"stringValue": row.direction}}));

    if let Some(ref agent) = row.agent_type {
        attributes.push(json!({"key": "agent.type", "value": {"stringValue": agent}}));
    }
    if let Some(tokens) = row.prompt_tokens {
        attributes.push(json!({"key": "llm.tokens.prompt", "value": {"intValue": tokens}}));
    }
    if let Some(tokens) = row.completion_tokens {
        attributes.push(json!({"key": "llm.tokens.completion", "value": {"intValue": tokens}}));
    }
    if let Some(tokens) = row.total_tokens {
        attributes.push(json!({"key": "llm.tokens.total", "value": {"intValue": tokens}}));
    }
    attributes.push(json!({"key": "llm.payload_size_bytes", "value": {"intValue": row.payload_size_bytes}}));

    if row.direction == "response" && !row.metadata.is_empty() {
        let meta = if sanitize { sanitize_body(&row.metadata) } else { row.metadata.clone() };
        attributes.push(json!({"key": "llm.response_metadata", "value": {"stringValue": meta}}));
    }

    let body = if let Some(ref payload) = row.payload {
        if sanitize { sanitize_body(payload) } else { payload.clone() }
    } else {
        String::new()
    };

    let time_unix_nano = chrono::DateTime::parse_from_rfc3339(&row.timestamp)
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0).to_string())
        .unwrap_or_else(|_| "0".to_string());

    json!({
        "timeUnixNano": time_unix_nano,
        "severityNumber": 9,
        "severityText": "INFO",
        "body": {
            "stringValue": body
        },
        "attributes": attributes
    })
}

/// 构建完整的 OTLP ExportLogsServiceRequest JSON
pub fn build_otlp_request(records: Vec<Value>) -> Value {
    json!({
        "resourceLogs": [
            {
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": "deepharness-gatewayd"}},
                        {"key": "service.version", "value": {"stringValue": env!("CARGO_PKG_VERSION")}},
                        {"key": "host.name", "value": {"stringValue": get_hostname()}}
                    ]
                },
                "scopeLogs": [
                    {
                        "scope": {"name": "gatewayd.audit"},
                        "logRecords": records
                    }
                ]
            }
        ]
    })
}

pub fn sanitize_body(content: &str) -> String {
    if content.len() > 64 {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let hash = hex::encode(hasher.finalize());
        format!("{}...{}...{}", &content[..16], &hash[..8], &content[content.len()-8..])
    } else {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }
}

fn get_hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string())
}
