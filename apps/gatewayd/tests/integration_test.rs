use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

static APP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const TEST_API_KEY: &str = "test-api-key-0123456789abcdef";

#[allow(clippy::await_holding_lock)]
async fn setup_app() -> (axum::Router, axum::Router) {
    // Serialise test setup to avoid races on process-wide environment variables
    // (GATEWAYD_API_KEY, GATEWAYD_DATA_DIR, XDG_DATA_HOME). The lock is released
    // once the test routers are built.
    let _guard = APP_LOCK.lock().unwrap();
    std::env::set_var("GATEWAYD_API_KEY", TEST_API_KEY);
    let (api, admin) = dh_gatewayd::server::build_test_app()
        .await
        .expect("failed to build test app");
    (api, admin)
}

#[tokio::test]
async fn health_check_returns_ok() {
    let (_api, admin) = setup_app().await;
    let response = admin
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn chat_endpoint_requires_api_key() {
    let (api, _admin) = setup_app().await;
    let response = api
        .oneshot(
            Request::post("/v1/chat/completions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn chat_endpoint_accepts_valid_api_key() {
    let (api, _admin) = setup_app().await;
    let response = api
        .oneshot(
            Request::post("/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", TEST_API_KEY))
                .body(Body::from(r#"{"model":"gpt-4o","messages":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // The request is validly authenticated but will fail upstream forwarding
    // because no API key is configured for the provider. We just verify it
    // passes gatewayd auth.
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_session_and_reject_invalid_workspace() {
    let (_api, admin) = setup_app().await;

    let create_session = Request::post("/sessions")
        .header("Authorization", format!("Bearer {}", TEST_API_KEY))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"id":"test-session"}"#))
        .unwrap();
    let response = admin.clone().oneshot(create_session).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["sessionId"].as_str(), Some("test-session"));

    let create_agent = Request::post("/sessions/test-session/agents")
        .header("Authorization", format!("Bearer {}", TEST_API_KEY))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"agent_key":"opencode","name":"test","work_directory":"/nonexistent/path"}"#))
        .unwrap();
    let response = admin.oneshot(create_agent).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
