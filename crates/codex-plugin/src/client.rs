use agent_core::process::stdio::StdioTransport;
use agent_core::process::transport::{Transport, TransportHandle};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio::time::{timeout, Duration};

use crate::constants::*;

type SharedHandle = Arc<TokioMutex<Option<Box<dyn TransportHandle>>>>;

/// A tiny JSON-RPC client over the Codex app-server stdio transport.
/// It supports sending requests, receiving responses by id, and forwarding
/// notifications to a channel.
pub struct CodexClient {
    handle: SharedHandle,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: Arc<AtomicU64>,
    notification_tx: mpsc::Sender<Value>,
}

impl CodexClient {
    pub fn new(_workspace: &str, notification_tx: mpsc::Sender<Value>) -> Self {
        Self {
            handle: Arc::new(TokioMutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            notification_tx,
        }
    }

    pub async fn start(&mut self, workspace: &str) -> Result<(), String> {
        let transport = StdioTransport::new(
            PROGRAM_CODEX,
            vec![
                APP_SERVER_CMD.into(),
                LISTEN_FLAG.into(),
                LISTEN_STDIO.into(),
            ],
            workspace.to_string(),
        );
        let raw_handle = transport
            .start()
            .await
            .map_err(|e| format!("{}: {}", ERR_START_FAILED, e))?;

        *self.handle.lock().await = Some(raw_handle);
        self.spawn_reader();

        // Send initialize request; app-server will emit capabilities and then
        // accept further requests. We do not block on the response here.
        self.notify(
            METHOD_INITIALIZE,
            json!({
                "capabilities": {},
                "clientInfo": { "name": "deepharness", "version": "0.1.0" }
            }),
        )
        .await?;

        // Notify the server that initialization is complete.
        self.notify(METHOD_INITIALIZED, json!({})).await?;
        Ok(())
    }

    /// Send a JSON-RPC request and await its response.
    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            self.pending.lock().unwrap().insert(id, tx);
        }

        let payload = json!({
            KEY_ID: id,
            KEY_METHOD: method,
            KEY_PARAMS: params,
        });

        self.send(payload).await?;

        match timeout(Duration::from_millis(REQUEST_TIMEOUT_MS), rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err("request channel closed".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(ERR_REQUEST_TIMEOUT.into())
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({
            KEY_METHOD: method,
            KEY_PARAMS: params,
        });
        self.send(payload).await
    }

    async fn send(&mut self, payload: Value) -> Result<(), String> {
        let mut guard = self.handle.lock().await;
        let handle = guard.as_mut().ok_or_else(|| ERR_NOT_INITIALIZED.to_string())?;
        handle.send(payload).await.map_err(|e| e.to_string())
    }

    pub async fn close(&mut self) {
        if let Some(mut handle) = self.handle.lock().await.take() {
            let _ = handle.close().await;
        }
    }

    fn spawn_reader(&mut self) {
        let handle = self.handle.clone();
        let pending = self.pending.clone();
        let notification_tx = self.notification_tx.clone();

        tokio::spawn(async move {
            loop {
                let next = {
                    let mut guard = handle.lock().await;
                    let Some(handle) = guard.as_mut() else {
                        break;
                    };
                    timeout(Duration::from_millis(RECEIVE_TIMEOUT_MS), handle.receive()).await
                };

                match next {
                    Ok(Ok(value)) => {
                        Self::dispatch(value, &pending, &notification_tx).await;
                    }
                    Ok(Err(_)) | Err(_) => {
                        // Timeout or transport error: keep polling until close.
                    }
                }
            }
        });
    }

    async fn dispatch(
        value: Value,
        pending: &Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
        notification_tx: &mpsc::Sender<Value>,
    ) {
        if let Some(id) = value.get(KEY_ID).and_then(|v| v.as_u64()) {
            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                let _ = tx.send(value);
                return;
            }
        }

        // No matching pending request: treat as notification.
        let _ = notification_tx.send(value).await;
    }
}
