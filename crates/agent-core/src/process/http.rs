//! HTTP transport for agent processes.
//!
//! This module implements a skeleton `Transport` over HTTP with a one-way
//! Server-Sent Events (SSE) stream. Outbound messages are currently a no-op
//! because HTTP session management is handled by consumers that create the
//! session externally.

use crate::process::transport::{Transport, TransportError, TransportHandle};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

/// SSE endpoint path appended to the agent base URL.
const SSE_ENDPOINT: &str = "/event";
/// HTTP header name used to request an event-stream response.
const SSE_ACCEPT_HEADER_NAME: &str = "Accept";
/// MIME type for Server-Sent Events.
const SSE_ACCEPT_HEADER_VALUE: &str = "text/event-stream";
/// Prefix for SSE `data:` lines carrying JSON payloads.
const SSE_DATA_PREFIX: &str = "data: ";
/// Buffer size for the internal channel that `receive()` reads from.
const CHANNEL_CAPACITY: usize = 1000;
/// Delay between SSE reconnection attempts.
const RETRY_DELAY_SECS: u64 = 3;
/// Prefix for log messages emitted by this module.
const LOG_PREFIX: &str = "[HttpTransport]";
/// Error message returned when `receive()` is called before SSE is connected.
const ERR_SSE_NOT_CONNECTED: &str = "SSE not connected";

pub struct HttpTransport {
    base_url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    /// Creates a new HTTP transport for the given agent base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Creates a new HTTP transport using the provided HTTP client.
    pub fn with_client(base_url: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.into(),
            client,
        }
    }

    /// Starts the transport and connects the SSE stream.
    ///
    /// Returns a [`TransportHandle`] that keeps the background SSE listener
    /// alive. Callers should hold onto the handle until the agent instance is
    /// stopped.
    pub async fn connect_sse(
        &self,
        instance_id: String,
        sender: mpsc::Sender<Value>,
    ) -> Result<Box<dyn TransportHandle>, TransportError> {
        let mut handle = HttpHandle {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
            receiver: None,
            sse_task: None,
        };
        handle.connect_sse(instance_id, sender);
        Ok(Box::new(handle))
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError> {
        Ok(Box::new(HttpHandle {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
            receiver: None,
            sse_task: None,
        }))
    }

    fn endpoint(&self) -> Option<String> {
        Some(self.base_url.clone())
    }
}

struct HttpHandle {
    base_url: String,
    client: reqwest::Client,
    /// Channel exposed via `receive()` so callers can pull one event at a time.
    receiver: Option<mpsc::Receiver<Value>>,
    /// Handle for the background SSE polling task; aborted on `close()`.
    sse_task: Option<JoinHandle<()>>,
}

#[async_trait]
impl TransportHandle for HttpHandle {
    async fn send(&mut self, payload: Value) -> Result<(), TransportError> {
        // HTTP sessions are created and managed by consumers outside this
        // transport, so there is no outbound message to send at this layer.
        let _ = payload;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Value, TransportError> {
        if let Some(ref mut rx) = self.receiver {
            rx.recv().await.ok_or(TransportError::Closed)
        } else {
            Err(TransportError::ReceiveFailed(ERR_SSE_NOT_CONNECTED.into()))
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(task) = self.sse_task.take() {
            task.abort();
        }
        self.receiver = None;
        Ok(())
    }
}

impl HttpHandle {
    /// Starts a background SSE listener for the given instance.
    pub fn connect_sse(&mut self, _instance_id: String, sender: mpsc::Sender<Value>) {
        let url = build_sse_url(&self.base_url);
        let client = self.client.clone();
        let (internal_tx, rx) = mpsc::channel::<Value>(CHANNEL_CAPACITY);
        self.receiver = Some(rx);

        let task = tokio::spawn(async move {
            loop {
                // Start each connection with a fresh byte buffer so stale
                // partial lines from a previous stream cannot corrupt the new
                // one after reconnection.
                let mut buffer = Vec::new();
                match connect_sse_stream(&client, &url).await {
                    Ok(stream) => {
                        process_sse_stream(
                            stream,
                            &mut buffer,
                            &internal_tx,
                            &sender,
                        )
                        .await;
                    }
                    Err(e) => {
                        log::warn!(
                            "{LOG_PREFIX}: SSE connect error: {e}, retrying..."
                        );
                    }
                }
                sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
            }
        });

        self.sse_task = Some(task);
    }
}

/// Builds the absolute SSE URL from the agent base URL.
fn build_sse_url(base_url: &str) -> String {
    format!("{base_url}{SSE_ENDPOINT}")
}

/// Opens an SSE connection and returns the byte stream.
async fn connect_sse_stream(
    client: &reqwest::Client,
    url: &str,
) -> reqwest::Result<impl Stream<Item = reqwest::Result<Bytes>> + Unpin> {
    let resp = client
        .get(url)
        .header(SSE_ACCEPT_HEADER_NAME, SSE_ACCEPT_HEADER_VALUE)
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.bytes_stream())
}

/// Consumes chunks from an established SSE byte stream until it ends or errors.
async fn process_sse_stream(
    mut stream: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
    buffer: &mut Vec<u8>,
    internal_tx: &mpsc::Sender<Value>,
    external_tx: &mpsc::Sender<Value>,
) {
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let values = parse_sse_chunk(buffer, &bytes);
                forward_values(values, internal_tx, external_tx).await;
            }
            Err(e) => {
                log::warn!("{LOG_PREFIX}: SSE chunk error: {e}, reconnecting...");
                break;
            }
        }
    }
}

/// Appends a new chunk to the byte buffer, extracts complete `data:` lines,
/// and returns the parsed JSON values.
///
/// SSE messages are delimited by newline characters. Because `reqwest` may
/// split the byte stream at arbitrary byte boundaries, multi-byte UTF-8
/// characters are only decoded once a complete line has arrived. The bytes
/// after the last newline are kept in `buffer` until the next chunk arrives.
fn parse_sse_chunk(buffer: &mut Vec<u8>, chunk: &Bytes) -> Vec<Value> {
    buffer.extend_from_slice(chunk);
    let mut values = Vec::new();

    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
        // Split off the line including the trailing newline; swap it into
        // `buffer` so the remaining bytes stay in `buffer` for the next loop.
        let mut line = buffer.split_off(pos + 1);
        std::mem::swap(buffer, &mut line);

        // Remove the trailing newline and any preceding carriage return.
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }

        let line_str = String::from_utf8_lossy(&line);
        if let Some(data) = line_str.strip_prefix(SSE_DATA_PREFIX) {
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                values.push(value);
            }
        }
    }

    values
}

/// Forwards parsed SSE values to both the internal receive channel and the
/// external sink supplied by the caller.
///
/// Two channels are used because `receive()` exposes a pull-style API to the
/// transport consumer, while the external `sender` lets the caller broadcast
/// or log the same events elsewhere without blocking `receive()`.
async fn forward_values(
    values: Vec<Value>,
    internal_tx: &mpsc::Sender<Value>,
    external_tx: &mpsc::Sender<Value>,
) {
    for value in values {
        // A send error means the consumer has dropped; the loop will exit on
        // the next receive or reconnection. Dropping the event is intentional
        // to avoid backpressure blocking the SSE reader.
        let _ = internal_tx.send(value.clone()).await;
        let _ = external_tx.send(value).await;
    }
}
