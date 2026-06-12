pub mod codec {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct JsonRpcRequest {
        pub jsonrpc: String,
        pub id: Option<Value>,
        pub method: String,
        #[serde(default)]
        pub params: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct JsonRpcResponse {
        pub jsonrpc: String,
        pub id: Option<Value>,
        #[serde(flatten)]
        pub result: JsonRpcResult,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum JsonRpcResult {
        Success { result: Value },
        Error { error: JsonRpcError },
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct JsonRpcError {
        pub code: i64,
        pub message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub data: Option<Value>,
    }

    impl JsonRpcRequest {
        pub fn new(method: &str, params: Value) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id: Some(Value::String(format!("req-{}", uuid::Uuid::new_v4()))),
                method: method.to_string(),
                params,
            }
        }

        pub fn is_notification(&self) -> bool {
            self.id.is_none()
        }
    }

    impl JsonRpcResponse {
        pub fn success(id: Option<Value>, result: Value) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id,
                result: JsonRpcResult::Success { result },
            }
        }

        pub fn error(id: Option<Value>, code: i64, message: &str, data: Option<Value>) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id,
                result: JsonRpcResult::Error {
                    error: JsonRpcError {
                        code,
                        message: message.to_string(),
                        data,
                    },
                },
            }
        }
    }
}

pub mod types {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InitializeRequest {
        pub protocol_version: String,
        pub capabilities: ClientCapabilities,
        pub client_info: Implementation,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClientCapabilities {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub sampling: Option<Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Implementation {
        pub name: String,
        pub version: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InitializeResult {
        pub protocol_version: String,
        pub capabilities: ServerCapabilities,
        pub server_info: Implementation,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServerCapabilities {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tools: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub resources: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub prompts: Option<Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Tool {
        pub name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        pub input_schema: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolResult {
        pub content: Vec<ToolContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub is_error: Option<bool>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum ToolContent {
        #[serde(rename = "text")]
        Text { text: String },
    }

    #[derive(Debug, Clone)]
    pub enum McpError {
        ProcessError(String),
        ProtocolError(String),
        RequestTimeout,
        NotInitialized,
    }

    impl std::fmt::Display for McpError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                McpError::ProcessError(msg) => write!(f, "MCP process error: {}", msg),
                McpError::ProtocolError(msg) => write!(f, "MCP protocol error: {}", msg),
                McpError::RequestTimeout => write!(f, "MCP request timeout"),
                McpError::NotInitialized => write!(f, "MCP client not initialized"),
            }
        }
    }

    impl std::error::Error for McpError {}
}

pub mod transport {
    use super::types::McpError;
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::{Child, ChildStdin, Command};
    use tokio::sync::mpsc;

    pub struct StdioTransport {
        stdin: ChildStdin,
        stdout_tx: mpsc::UnboundedSender<String>,
        _child: Child,
    }

    impl StdioTransport {
        pub async fn spawn(command: &str, args: &[String], workspace: &str) -> Result<(Self, mpsc::UnboundedReceiver<String>), McpError> {
            let mut cmd = Command::new(command);
            cmd.args(args)
                .current_dir(workspace)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            
            let mut child = cmd.spawn().map_err(|e| McpError::ProcessError(e.to_string()))?;
            
            let stdin = child.stdin.take().ok_or_else(|| McpError::ProcessError("Failed to open stdin".to_string()))?;
            let stdout = child.stdout.take().ok_or_else(|| McpError::ProcessError("Failed to open stdout".to_string()))?;
            
            let (stdout_tx, stdout_rx) = mpsc::unbounded_channel::<String>();
            let stdout_tx_clone = stdout_tx.clone();
            
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                
                while let Ok(Some(line)) = lines.next_line().await {
                    if stdout_tx_clone.send(line).is_err() {
                        break;
                    }
                }
            });
            
            if let Some(stderr) = child.stderr.take() {
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    
                    while let Ok(Some(line)) = lines.next_line().await {
                        log::warn!("MCP stderr: {}", line);
                    }
                });
            }
            
            Ok((Self {
                stdin,
                stdout_tx,
                _child: child,
            }, stdout_rx))
        }
        
        pub async fn send(&mut self, message: String) -> Result<(), McpError> {
            let json = format!("{}\n", message);
            self.stdin.write_all(json.as_bytes()).await
                .map_err(|e| McpError::ProcessError(e.to_string()))?;
            self.stdin.flush().await
                .map_err(|e| McpError::ProcessError(e.to_string()))?;
            Ok(())
        }
        
        pub fn subscribe(&self) -> mpsc::UnboundedSender<String> {
            self.stdout_tx.clone()
        }
    }
}

pub mod client {
    use super::codec::{JsonRpcRequest, JsonRpcResponse};
    use super::transport::StdioTransport;
    use super::types::*;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    pub struct McpClient {
        transport: Arc<tokio::sync::Mutex<StdioTransport>>,
        request_id: AtomicU64,
        pending: Arc<tokio::sync::Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
        notification_handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(Value) + Send>>>>,
        initialized: Arc<Mutex<bool>>,
    }

    impl McpClient {
        pub async fn spawn(command: &str, args: &[String], workspace: &str) -> Result<Self, McpError> {
            let (transport, mut stdout_rx) = StdioTransport::spawn(command, args, workspace).await?;
            let transport = Arc::new(tokio::sync::Mutex::new(transport));
            let pending: Arc<tokio::sync::Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
                Arc::new(tokio::sync::Mutex::new(HashMap::new()));
            let notification_handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(Value) + Send>>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let pending_clone = pending.clone();
            let handlers_clone = notification_handlers.clone();

            tokio::spawn(async move {
                while let Some(line) = stdout_rx.recv().await {
                    match serde_json::from_str::<JsonRpcResponse>(&line) {
                        Ok(response) => {
                            if let Some(id) = response.id.as_ref().and_then(|v| v.as_u64()) {
                                if let Some(sender) = pending_clone.lock().await.remove(&id) {
                                    let _ = sender.send(response);
                                }
                            }
                        }
                        Err(_) => {
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
                super::codec::JsonRpcResult::Success { result } => {
                    let init_result: InitializeResult = serde_json::from_value(result)
                        .map_err(|e| McpError::ProtocolError(e.to_string()))?;

                    *self.initialized.lock().unwrap() = true;

                    let notification = JsonRpcRequest {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        method: "notifications/initialized".to_string(),
                        params: json!({}),
                    };
                    self.send_notification(notification).await?;

                    Ok(init_result)
                }
                super::codec::JsonRpcResult::Error { error } => {
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
                super::codec::JsonRpcResult::Success { result } => {
                    let tool_result: ToolResult = serde_json::from_value(result)
                        .map_err(|e| McpError::ProtocolError(e.to_string()))?;
                    Ok(tool_result)
                }
                super::codec::JsonRpcResult::Error { error } => {
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
                let mut pending = self.pending.lock().await;
                pending.insert(id, tx);
            }

            let json = serde_json::to_string(&request)
                .map_err(|e| McpError::ProtocolError(e.to_string()))?;

            {
                let mut transport = self.transport.lock().await;
                transport.send(json).await?;
            }

            match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(_)) => Err(McpError::ProtocolError("Request cancelled".to_string())),
                Err(_) => {
                    self.pending.lock().await.remove(&id);
                    Err(McpError::RequestTimeout)
                }
            }
        }

        async fn send_notification(&self, request: JsonRpcRequest) -> Result<(), McpError> {
            let json = serde_json::to_string(&request)
                .map_err(|e| McpError::ProtocolError(e.to_string()))?;

            let mut transport = self.transport.lock().await;
            transport.send(json).await
        }

        pub async fn shutdown(&self) -> Result<(), McpError> {
            Ok(())
        }
    }
}
