#![allow(dead_code)]

use crate::agui::types::RunAgentInput;
use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};

pub async fn session_events_handler(
    ws: WebSocketUpgrade,
    State(state): State<crate::ApiState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(socket: WebSocket, state: crate::ApiState, session_id: String) {
    let (mut sender, mut receiver) = socket.split();

    let mut rx = match state.session_manager.subscribe(&session_id) {
        Some(rx) => rx,
        None => {
            let err = serde_json::json!({ "type": "RUN_ERROR", "message": "session not found" })
                .to_string();
            let _ = sender.send(Message::Text(err.into())).await;
            return;
        }
    };

    let service = match state.agent_service.as_ref() {
        Some(s) => s,
        None => {
            let err = serde_json::json!({ "type": "RUN_ERROR", "message": "Agent runtime not available" })
                .to_string();
            let _ = sender.send(Message::Text(err.into())).await;
            return;
        }
    };

    // Forward broadcast events to WebSocket client.
    let forward_task = tokio::spawn(async move {
        let mut sender = sender;
        while let Ok(event) = rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap_or_default();
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read incoming RunAgentInput from client.
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<RunAgentInput>(&text) {
                Ok(input) => {
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
    }

    forward_task.abort();
}
