#![allow(dead_code)]

use crate::agui::types::RunAgentInput;
use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const MAX_WS_CONNECTIONS: usize = 1024;
const WS_HEARTBEAT_INTERVAL_SECS: u64 = 30;
const WS_CLIENT_TIMEOUT_SECS: u64 = 60;

pub async fn session_events_handler(
    ws: WebSocketUpgrade,
    State(state): State<crate::ApiState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(mut socket: WebSocket, state: crate::ApiState, session_id: String) {
    let current = state.ws_connections.fetch_add(1, Ordering::Relaxed);
    if current >= MAX_WS_CONNECTIONS {
        state.ws_connections.fetch_sub(1, Ordering::Relaxed);
        tracing::warn!(
            "[websocket] connection limit reached ({}/{}), closing session={}",
            current,
            MAX_WS_CONNECTIONS,
            session_id
        );
        let _ = socket.close().await;
        return;
    }

    let (mut sender, mut receiver) = socket.split();

    let mut rx = match state.session_manager.subscribe(&session_id).await {
        Some(rx) => rx,
        None => {
            let err = serde_json::json!({ "type": "RUN_ERROR", "message": "session not found" })
                .to_string();
            let _ = sender.send(Message::Text(err.into())).await;
            state.ws_connections.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    };

    let service = match state.agent_service.as_ref() {
        Some(s) => s,
        None => {
            let err = serde_json::json!({ "type": "RUN_ERROR", "message": "Agent runtime not available" })
                .to_string();
            let _ = sender.send(Message::Text(err.into())).await;
            state.ws_connections.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    };

    let last_activity = Arc::new(std::sync::atomic::AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    ));

    // Forward broadcast events to WebSocket client.
    let forward_last_activity = last_activity.clone();
    let forward_session_id = session_id.clone();
    let forward_task = tokio::spawn(async move {
        let mut sender = sender;
        let mut heartbeat = tokio::time::interval(Duration::from_secs(WS_HEARTBEAT_INTERVAL_SECS));
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let last = forward_last_activity.load(Ordering::Relaxed);
                    if now.saturating_sub(last) > WS_CLIENT_TIMEOUT_SECS {
                        tracing::warn!("[websocket] client timeout for session={}", forward_session_id);
                        break;
                    }
                    if sender.send(Message::Ping(vec![].into())).await.is_err() {
                        break;
                    }
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            forward_last_activity.store(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                Ordering::Relaxed,
                            );
                            let msg = serde_json::to_string(&event).unwrap_or_default();
                            if sender.send(Message::Text(msg.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                "[websocket] consumer lagged, dropped {} events; closing connection",
                                skipped
                            );
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // Read incoming RunAgentInput from client with a timeout.
    let read_timeout = Duration::from_secs(WS_CLIENT_TIMEOUT_SECS);
    loop {
        match tokio::time::timeout(read_timeout, receiver.next()).await {
            Ok(Some(Ok(msg))) => {
                last_activity.store(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    Ordering::Relaxed,
                );
                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<RunAgentInput>(&text) {
                            Ok(input) => {
                                // 收到用户输入，刷新 session 空闲计时器。
                                state.session_manager.touch_session(&session_id).await;
                                if let Err(e) = state
                                    .session_manager
                                    .start_run(&session_id, input, service)
                                    .await
                                {
                                    tracing::warn!("failed to start run: {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::warn!("invalid RunAgentInput: {}", e);
                            }
                        }
                    }
                    Message::Pong(_) => {}
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Ok(Some(Err(e))) => {
                tracing::warn!("[websocket] receive error for session={}: {}", session_id, e);
                break;
            }
            Ok(None) => break,
            Err(_) => {
                tracing::warn!("[websocket] read timeout for session={}", session_id);
                break;
            }
        }
    }

    forward_task.abort();
    state.ws_connections.fetch_sub(1, Ordering::Relaxed);
}
