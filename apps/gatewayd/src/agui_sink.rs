#![allow(dead_code)]

use crate::agui::mapper::AguiMapper;
use crate::agui::types::Event;
use crate::session::SessionManager;
use agent_core::event_sink::EventSink;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// A unit of work enqueued by [`AguiEventSink::emit`] and drained serially by a
/// single long-lived consumer task (see [`consumer_loop`]).
///
/// The mapper has already run before the job is enqueued, so each job carries
/// the fully-formed [`Event`]s that just need to be routed to the right session
/// and broadcast.
enum EmitJob {
    Broadcast {
        instance_id: String,
        conversation_id: String,
        event_type: String,
        events: Vec<Event>,
    },
}

/// Routes agent JSON-RPC notifications into AG-UI events for the right session.
///
/// ## Ordering guarantee
///
/// All events flow through a single [`mpsc::unbounded_channel`] into one
/// consumer task ([`consumer_loop`]). Because:
/// 1. [`mpsc::UnboundedSender::send`] is synchronous and FIFO,
/// 2. there is exactly one consumer, and
/// 3. the consumer awaits each job to completion before polling the next,
///
/// the broadcast order is strictly the `emit` call order. This is critical for
/// high-frequency token streams (e.g. opencode `agent.token` at 30-100μs
/// intervals) where the previous per-emit `runtime.spawn` design let tokio
/// reorder independent tasks, scrambling the SSE byte stream.
pub struct AguiEventSink {
    mappers: Arc<Mutex<HashMap<String, AguiMapper>>>,
    tx: mpsc::UnboundedSender<EmitJob>,
}

impl AguiEventSink {
    pub fn new(session_manager: SessionManager, runtime: tokio::runtime::Handle) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<EmitJob>();
        // Spawn the single serial consumer that owns session resolution and
        // broadcast. Dropping all `tx` clones (i.e. dropping the sink) causes
        // `rx.recv()` to return `None` and the task exits cleanly.
        runtime.spawn(consumer_loop(rx, session_manager));
        Self {
            mappers: Arc::new(Mutex::new(HashMap::new())),
            tx,
        }
    }

    fn mapper_for(&self, instance_id: &str) -> AguiMapper {
        self.mappers
            .lock()
            .unwrap()
            .entry(instance_id.to_string())
            .or_default()
            .clone()
    }

    fn update_mapper(&self, instance_id: &str, mapper: AguiMapper) {
        self.mappers
            .lock()
            .unwrap()
            .insert(instance_id.to_string(), mapper);
    }
}

impl EventSink for AguiEventSink {
    fn emit(&self, event_type: &str, payload: Value) {
        let instance_id = payload
            .get("instance_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let conversation_id = payload
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Mapping is synchronous and stateful (per-instance AguiMapper tracks
        // current_message_id / thinking state), so it MUST run inline in the
        // calling thread to preserve Start/Content/End ordering. Only the
        // session-resolution + broadcast step is deferred to the consumer.
        let mut mapper = self.mapper_for(&instance_id);
        let events = mapper.map(event_type, &payload);
        self.update_mapper(&instance_id, mapper);

        if events.is_empty() {
            return;
        }

        let job = EmitJob::Broadcast {
            instance_id,
            conversation_id,
            event_type: event_type.to_string(),
            events,
        };
        // UnboundedSender::send is synchronous, non-blocking, and FIFO.
        // A send error here means the consumer task has exited (process
        // shutdown); drop the event rather than panicking.
        if self.tx.send(job).is_err() {
            tracing::warn!(
                "[agui-sink] event_sink channel closed; dropping event={event_type}"
            );
        }
    }
}

/// Single-consumer loop that resolves the target session and broadcasts events.
///
/// There is exactly one instance of this task per [`AguiEventSink`]. It awaits
/// each job to completion (including the async `session_for_instance` /
/// `broadcast` calls) before polling the next, so the broadcast order is the
/// channel enqueue order, which equals the `emit` call order.
async fn consumer_loop(
    mut rx: mpsc::UnboundedReceiver<EmitJob>,
    session_manager: SessionManager,
) {
    while let Some(job) = rx.recv().await {
        let EmitJob::Broadcast {
            instance_id,
            conversation_id,
            event_type,
            events,
        } = job;

        let log_conversation_id = conversation_id.clone();
        // When conversation_id is present it IS the session_id; otherwise fall
        // back to looking up which session owns this instance.
        let session_id = if conversation_id.is_empty() {
            match session_manager.session_for_instance(&instance_id).await {
                Some(sid) => sid,
                None => {
                    tracing::warn!(
                        "[agui-sink] DROPPING event={event_type} - no session for instance_id={instance_id}"
                    );
                    continue;
                }
            }
        } else {
            conversation_id
        };

        tracing::info!(
            "[agui-sink] event={event_type} instance={instance_id} conv_id={log_conversation_id} routing_to={session_id}"
        );

        for event in events {
            session_manager.broadcast(&session_id, event).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agui::mapper::{METHOD_DONE, METHOD_TOKEN};
    use crate::agui::types::Event;
    use serde_json::json;

    /// Number of token events emitted in the ordering test. Kept below the
    /// broadcast channel capacity (1024) so a slow test receiver does not get
    /// `Lagged` while the consumer drains the queue.
    const ORDERING_TEST_TOKEN_COUNT: usize = 800;

    /// Emit a high-frequency token stream and assert the receiver observes the
    /// deltas in the exact emit order. This is the regression test for the
    /// per-emit `runtime.spawn` reordering bug.
    #[tokio::test(flavor = "current_thread")]
    async fn test_emit_order_preserved_for_high_frequency_tokens() {
        let session_manager = SessionManager::new();
        let session_id = session_manager
            .create_session(Some("ordering-test-session".to_string()), None)
            .await;

        let sink = AguiEventSink::new(
            session_manager.clone(),
            tokio::runtime::Handle::current(),
        );

        // Subscribe before emitting so we don't miss the first events.
        let mut rx = session_manager
            .subscribe(&session_id)
            .await
            .expect("session should exist");

        // Emit N tokens back-to-back. Each carries a sequential index so we can
        // verify ordering on the receiving side. conversation_id is set to the
        // session_id so the consumer uses it directly (no instance lookup).
        let expected_deltas: Vec<String> = (0..ORDERING_TEST_TOKEN_COUNT)
            .map(|i| format!("tok-{i:04}"))
            .collect();
        for delta in &expected_deltas {
            sink.emit(
                METHOD_TOKEN,
                json!({
                    "instance_id": "inst-ordering",
                    "conversation_id": session_id,
                    "text": delta,
                }),
            );
        }
        // Emit `agent.done` to close the message; receiving `TextMessageEnd`
        // tells us the consumer has finished draining all token events.
        sink.emit(
            METHOD_DONE,
            json!({
                "instance_id": "inst-ordering",
                "conversation_id": session_id,
            }),
        );

        // Collect received deltas until we see TextMessageEnd. We apply a
        // timeout so a logic bug fails fast instead of hanging.
        let mut received: Vec<String> = Vec::new();
        let collect = async {
            loop {
                match rx.recv().await {
                    Ok(event) => match event {
                        Event::TextMessageContent { delta, .. } => received.push(delta),
                        Event::TextMessageEnd { .. } => break,
                        _ => {}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        panic!("receiver lagged by {n} events; test token count too high");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        panic!("broadcast channel closed before TextMessageEnd");
                    }
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), collect)
            .await
            .expect("timed out waiting for events");

        assert_eq!(
            received.len(),
            ORDERING_TEST_TOKEN_COUNT,
            "should receive exactly the emitted number of content deltas"
        );
        assert_eq!(
            received, expected_deltas,
            "deltas must arrive in emit order; any reordering indicates the \
             per-emit spawn regression returned"
        );
    }
}
