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
use uuid;

pub async fn run_handler(
    State(state): State<crate::ApiState>,
    Path(session_id): Path<String>,
    axum::Json(input): axum::Json<RunAgentInput>,
) -> Result<Sse<AguiEventStream>, (StatusCode, axum::Json<Value>)> {
    let run_id = input.run_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let start = std::time::Instant::now();
    tracing::info!(
        "[gatewayd] run={} POST /sessions/{}/runs received",
        run_id,
        session_id
    );

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

    tracing::info!(
        "[gatewayd] run={} start_run completed after {:?}, returning SSE stream",
        run_id,
        start.elapsed()
    );

    let stream = AguiEventStream::new(rx, run_id.clone(), start);
    Ok(Sse::new(stream))
}

/// Wraps a broadcast receiver as an SSE stream.
pub struct AguiEventStream {
    inner: Pin<Box<dyn Stream<Item = Result<SseEvent, Infallible>> + Send>>,
}

impl AguiEventStream {
    fn new(rx: broadcast::Receiver<Event>, run_id: String, start: std::time::Instant) -> Self {
        let first_event = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stream = futures_util::stream::unfold((rx, first_event, run_id, start), |(mut rx, first_event, run_id, start)| async move {
            match rx.recv().await {
                Ok(event) => {
                    if !first_event.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let event_type = serde_json::to_value(&event)
                            .ok()
                            .and_then(|v| v.get("type").cloned())
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_else(|| "unknown".to_string());
                        tracing::info!(
                            "[gatewayd] run={} first event emitted after {:?}: type={}",
                            run_id,
                            start.elapsed(),
                            event_type
                        );
                    }
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    let sse_event = SseEvent::default().data(data);
                    Some((Ok(sse_event), (rx, first_event, run_id, start)))
                }
                Err(_) => None,
            }
        });
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl Stream for AguiEventStream {
    type Item = Result<SseEvent, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
