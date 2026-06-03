use super::transport::StdioTransport;
use super::types::*;
use crate::gateway::codec::{JsonRpcRequest, JsonRpcResponse};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub struct McpClient {
    transport: Arc<Mutex<StdioTransport>>,
    request_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(Value) + Send>>>>,
    initialized: Arc<Mutex<bool>>,
}

impl McpClient {
    pub async fn spawn(command: &str, args: &[String], workspace: &str) -> Result<Self, McpError> {
        let (transport, mut stdout_rx) = StdioTransport::spawn(command, args, workspace).await?;
        let transport = Arc::new(Mutex::new(transport));
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> = Arc::new(Mutex::new(HashMap::new()));
        let notification_handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(Value) + Send>>>> = Arc::new(Mutex::new(HashMap::new()));
        
        let pending_clone = pending.clone();
        let handlers_clone = notification_handlers.clone();
        
        // Spawn response handler task
        tokio::spawn(async move {
            while let Some(line) = stdout_rx.recv().await {
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(response) => {
                        if let Some(id) = response.id.as_ref().and_then(|v| v.as_u64()) {
                            if let Some(sender) = pending_clone.lock().unwrap().remove(&id) {
                                let _ = sender.send(response);
                            }
                        }
                    }
                    Err(_) => {
                        // Try as notification
                        if let Ok(notification) = serde_json::from_str::<Value>(&line) {
                            if let Some(method) = notification.get("method").and_then(|v| v.as_str()) {
                                let handlers = handlers_clone.lock().unwrap();
                                if let Some(handler) = handlers.get(method) {
                                    if let Some(params) = notification.get("params") {
                                        handler(params.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        
        Ok(Self {
            transport,
            request_id: AtomicU64::new(1),
            pending,
            notification_handlers,
            initialized: Arc::new(Mutex::new(false)),
        })
    }
    
    pub async fn initialize(&self) -> Result<InitializeResult, McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(0)),
            method: "initialize".to_string(),
            params: json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "deepharness-desktop",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        };
        
        let response = self.send_request(request).await?;
        
        match response.result {
            crate::gateway::codec::JsonRpcResult::Success { result } => {
                let init_result: InitializeResult = serde_json::from_value(result)
                    .map_err(|e| McpError::ProtocolError(e.to_string()))?;
                
                *self.initialized.lock().unwrap() = true;
                
                // Send initialized notification
                let notification = JsonRpcRequest {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    method: "notifications/initialized".to_string(),
                    params: json!({}),
                };
                self.send_notification(notification).await?;
                
                Ok(init_result)
            }
            crate::gateway::codec::JsonRpcResult::Error { error } => {
                Err(McpError::ProtocolError(error.message))
            }
        }
    }
    
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<ToolResult, McpError> {
        if !*self.initialized.lock().unwrap() {
            return Err(McpError::NotInitialized);
        }
        
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(self.request_id.fetch_add(1, Ordering::SeqCst))),
            method: "tools/call".to_string(),
            params: json!({
                "name": name,
                "arguments": arguments
            }),
        };
        
        let response = self.send_request(request).await?;
        
        match response.result {
            crate::gateway::codec::JsonRpcResult::Success { result } => {
                let tool_result: ToolResult = serde_json::from_value(result)
                    .map_err(|e| McpError::ProtocolError(e.to_string()))?;
                Ok(tool_result)
            }
            crate::gateway::codec::JsonRpcResult::Error { error } => {
                Err(McpError::ProtocolError(error.message))
            }
        }
    }
    
    pub fn on_notification<F>(&self, method: &str, handler: F)
    where
        F: Fn(Value) + Send + 'static,
    {
        let mut handlers = self.notification_handlers.lock().unwrap();
        handlers.insert(method.to_string(), Box::new(handler));
    }
    
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let id = request.id.as_ref().and_then(|v| v.as_u64()).unwrap_or(0);
        let (tx, rx) = oneshot::channel();
        
        {
            let mut pending = self.pending.lock().unwrap();
            pending.insert(id, tx);
        }
        
        let json = serde_json::to_string(&request)
            .map_err(|e| McpError::ProtocolError(e.to_string()))?;
        
        {
            let mut transport = self.transport.lock().unwrap();
            transport.send(json).await?;
        }
        
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(McpError::ProtocolError("Request cancelled".to_string())),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(McpError::RequestTimeout)
            }
        }
    }
    
    async fn send_notification(&self, request: JsonRpcRequest) -> Result<(), McpError> {
        let json = serde_json::to_string(&request)
            .map_err(|e| McpError::ProtocolError(e.to_string()))?;
        
        let mut transport = self.transport.lock().unwrap();
        transport.send(json).await
    }
    
    pub async fn shutdown(&self) -> Result<(), McpError> {
        // Send exit notification or just drop
        Ok(())
    }
}
