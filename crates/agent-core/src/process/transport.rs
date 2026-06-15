use async_trait::async_trait;
use serde_json::Value;
use std::fmt;

#[derive(Debug)]
pub enum TransportError {
    ProcessStart(String),
    ProcessExit(String),
    SendFailed(String),
    ReceiveFailed(String),
    Closed,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::ProcessStart(msg) => write!(f, "process start failed: {msg}"),
            TransportError::ProcessExit(msg) => write!(f, "process exit failed: {msg}"),
            TransportError::SendFailed(msg) => write!(f, "send failed: {msg}"),
            TransportError::ReceiveFailed(msg) => write!(f, "receive failed: {msg}"),
            TransportError::Closed => write!(f, "transport closed"),
        }
    }
}

impl std::error::Error for TransportError {}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError>;
    fn endpoint(&self) -> Option<String>;
}

#[async_trait]
pub trait TransportHandle: Send + Sync {
    async fn send(&mut self, payload: Value) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<Value, TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}
