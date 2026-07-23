#![allow(dead_code)]

use crate::agui::types::{BaseEvent, Event, RunAgentInput};
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

use crate::ApiState;

/// SSE keepalive 间隔（秒）：agent 长时间无事件输出时，
/// 每 15 秒发送一条注释行，避免下游客户端因空闲超时断开连接。
const SSE_KEEPALIVE_INTERVAL_SECS: u64 = 15;

pub async fn chat_handler(
    State(state): State<ApiState>,
    Path(session_id): Path<String>,
    axum::Json(input): axum::Json<RunAgentInput>,
) -> Result<
    Sse<impl Stream<Item = Result<SseEvent, Infallible>>>,
    (StatusCode, axum::Json<Value>),
> {
    let run_id = input.run_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let start = std::time::Instant::now();
    tracing::info!(
        "[gatewayd] run={} POST /sessions/{}/chat received",
        run_id,
        session_id
    );

    let service = state.agent_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "Agent runtime not available" })),
        )
    })?;

    // 显式校验 session 是否存在，不存在直接返回 404。
    let rx = state
        .session_manager
        .subscribe(&session_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({
                    "error": "session not found",
                    "session_id": session_id,
                })),
            )
        })?;

    // 若 session 下尚无 agent 实例，且请求未携带 agent_key，则报错；
    // 若携带了 agent_key，start_run 内部会自动挂载对应插件实例。
    let session = state.session_manager.get_session(&session_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "error": "session not found" })),
        )
    })?;
    if session.instances().is_empty() && input.agent_key.as_deref().filter(|s| !s.is_empty()).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "session has no agent instance; provide agent_key in the request to auto-create one",
                "session_id": session_id,
            })),
        ));
    }

    let session_mgr = state.session_manager.clone();
    let agent_svc = service.clone();
    let run_id_bg = run_id.clone();
    let session_id_bg = session_id.clone();
    tokio::spawn(async move {
        match session_mgr.start_run(&session_id_bg, input, agent_svc.as_ref()).await {
            Ok(_) => {}
            Err(e) => {
                tracing::error!(
                    "[gatewayd] run={} start_run error: {:?}",
                    run_id_bg,
                    e
                );
                // 向 session 事件通道广播终态 RUN_ERROR，告知 SSE 客户端 run 已失败，
                // 否则客户端会一直等待后续事件直到超时。
                session_mgr
                    .broadcast(
                        &session_id_bg,
                        Event::RunError {
                            base: BaseEvent {
                                timestamp: Some(crate::session::now()),
                                raw_event: None,
                            },
                            message: format!("run={} start_run failed: {}", run_id_bg, e),
                            code: None,
                        },
                    )
                    .await;
            }
        }
    });

    tracing::info!(
        "[gatewayd] run={} returning SSE stream (start_run spawned in background)",
        run_id,
    );

    let stream = AguiEventStream::new(rx, run_id.clone(), start);
    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(SSE_KEEPALIVE_INTERVAL_SECS))
            .text("ping"),
    ))
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
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        "[sse] run={} consumer lagged, dropped {} events; closing stream",
                        run_id,
                        skipped
                    );
                    None
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
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
