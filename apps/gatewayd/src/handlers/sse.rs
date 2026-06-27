#![allow(dead_code)]

use crate::agui::types::{Event, RunAgentInput};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Sse, sse::Event as SseEvent},
};
use futures_util::stream::Stream;
use serde_json::Value;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;

pub async fn run_handler(
    State(state): State<crate::ApiState>,
    Path(session_id): Path<String>,
    axum::Json(input): axum::Json<RunAgentInput>,
) -> Result<Sse<AguiEventStream>, (StatusCode, axum::Json<Value>)> {
    let service = state.agent_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "Agent runtime not available" })),
        )
    })?;

    let rx = state
        .session_manager
        .subscribe(&session_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({ "error": "session not found" })),
            )
        })?;

    state
        .session_manager
        .start_run(&session_id, input, service)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let stream = AguiEventStream::new(rx);
    Ok(Sse::new(stream))
}

pub struct AguiEventStream {
    rx: broadcast::Receiver<Event>,
    recv_fut: Option<
        Pin<
            Box<
                dyn std::future::Future<Output = Result<Event, broadcast::error::RecvError>> + Send,
            >,
        >,
    >,
}

impl AguiEventStream {
    fn new(rx: broadcast::Receiver<Event>) -> Self {
        Self { rx, recv_fut: None }
    }
}

impl Stream for AguiEventStream {
    type Item = Result<SseEvent, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.recv_fut.is_none() {
            let mut rx = self.rx.resubscribe();
            self.recv_fut = Some(Box::pin(async move { rx.recv().await }));
        }

        let recv_fut = self.recv_fut.as_mut().unwrap();
        match recv_fut.as_mut().poll(cx) {
            Poll::Ready(Ok(event)) => {
                self.recv_fut = None;
                let data = serde_json::to_string(&event).unwrap_or_default();
                Poll::Ready(Some(Ok(SseEvent::default().data(data))))
            }
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
