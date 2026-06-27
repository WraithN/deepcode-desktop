use super::transform::{sanitize_body, transform_audit_log};
use dh_db::AuditLogRow;

fn make_test_row() -> AuditLogRow {
    AuditLogRow {
        rowid: 1,
        id: "test-id".into(),
        session_id: "sess-1".into(),
        request_id: "req-1".into(),
        direction: "request".into(),
        provider: "openai".into(),
        model: "gpt-4".into(),
        agent_type: Some("opencode".into()),
        payload: Some("Hello world this is a test message".into()),
        payload_size_bytes: 36,
        prompt_tokens: Some(10),
        completion_tokens: None,
        total_tokens: Some(10),
        timestamp: "2026-06-10T10:00:00Z".into(),
        metadata: "{}".into(),
    }
}

#[test]
fn test_transform_basic() {
    let row = make_test_row();
    let log = transform_audit_log(&row, false);

    assert!(log.get("timeUnixNano").is_some());
    let body = log["body"]["stringValue"].as_str().unwrap();
    assert_eq!(body, "Hello world this is a test message");
}

#[test]
fn test_transform_sanitize_short() {
    let row = make_test_row();
    let log = transform_audit_log(&row, true);

    let body = log["body"]["stringValue"].as_str().unwrap();
    assert_eq!(body.len(), 64); // SHA-256 hex
    assert!(!body.contains("Hello"));
}

#[test]
fn test_sanitize_long() {
    let long = "a".repeat(100);
    let result = sanitize_body(&long);
    assert!(result.starts_with("aaaaaaaaaaaaaaaa..."));
    assert!(result.ends_with("...aaaaaaaa"));
    assert_ne!(result.len(), 64);
}

#[test]
fn test_transform_attributes() {
    let row = make_test_row();
    let log = transform_audit_log(&row, false);

    let attrs = log["attributes"].as_array().unwrap();
    let keys: Vec<_> = attrs.iter().map(|a| a["key"].as_str().unwrap()).collect();

    assert!(keys.contains(&"audit.log_id"));
    assert!(keys.contains(&"llm.model"));
    assert!(keys.contains(&"llm.tokens.prompt"));
}
