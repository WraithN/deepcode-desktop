use super::codec::{JsonRpcRequest, JsonRpcResponse};
use super::transport::{McpTransport, StdioTransport};
use super::types::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// client 层 notification handler 表：method -> 回调。
type NotificationHandlerMap = HashMap<String, Box<dyn Fn(Value) + Send>>;

pub struct McpClient {
    // transport 现在直接用 Arc<StdioTransport>（trait 方法均为 &self）。
    // id 路由 / pending 表已下沉到 StdioTransport 内部。
    transport: Arc<StdioTransport>,
    request_id: AtomicU64,
    // notification 分发：client 层通过 on_notification 注册 handler，
    // transport 收到无 id 消息时回调此处的 dispatch 闭包。
    notification_handlers: Arc<Mutex<NotificationHandlerMap>>,
    initialized: Arc<Mutex<bool>>,
}

impl McpClient {
    pub async fn spawn(command: &str, args: &[String], env: &std::collections::HashMap<String, String>, workspace: &str) -> Result<Self, McpError> {
        let transport = StdioTransport::spawn(command, args, env, workspace).await?;
        let notification_handlers: Arc<Mutex<NotificationHandlerMap>> =
            Arc::new(Mutex::new(HashMap::new()));

        // 注册 transport 级 notification handler：
        // transport 在 stdout 中收到无 id 消息时回调，client 层按 method 分发到对应 handler。
        let handlers_clone = notification_handlers.clone();
        transport.set_notification_handler(Box::new(move |line: String| {
            dispatch_notification(&line, &handlers_clone);
        }));

        Ok(Self {
            transport: Arc::new(transport),
            request_id: AtomicU64::new(1),
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
            super::codec::JsonRpcResult::Error { error } => {
                Err(McpError::ProtocolError(error.message))
            }
        }
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        if !*self.initialized.lock().unwrap() {
            return Err(McpError::NotInitialized);
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(self.request_id.fetch_add(1, Ordering::SeqCst))),
            method: "tools/list".to_string(),
            params: json!({}),
        };

        let response = self.send_request(request).await?;

        match response.result {
            super::codec::JsonRpcResult::Success { result } => {
                let list_result: super::types::ListToolsResult = serde_json::from_value(result)
                    .map_err(|e| McpError::ProtocolError(e.to_string()))?;
                Ok(list_result.tools)
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

    /// 发送 JSON-RPC 请求并等待响应。
    ///
    /// 委托给 `transport.send_request(json)` 拿回响应字符串，再反序列化为
    /// `JsonRpcResponse`。id 路由 / pending 表 / 超时管理均由 transport 层处理。
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let json = serde_json::to_string(&request)
            .map_err(|e| McpError::ProtocolError(e.to_string()))?;

        let response_str = self.transport.send_request(json).await?;

        serde_json::from_str::<JsonRpcResponse>(&response_str)
            .map_err(|e| McpError::ProtocolError(e.to_string()))
    }

    async fn send_notification(&self, request: JsonRpcRequest) -> Result<(), McpError> {
        let json = serde_json::to_string(&request)
            .map_err(|e| McpError::ProtocolError(e.to_string()))?;

        self.transport.send(json).await
    }

    pub async fn is_alive(&self) -> bool {
        self.transport.is_alive().await
    }

    pub async fn shutdown(&self) -> Result<(), McpError> {
        self.transport.close().await
    }
}

/// 分发 notification 到 client 层注册的 handler。
///
/// 由 transport 的 notification_handler 回调调用：
/// 解析 JSON 提取 method，在 `notification_handlers` 中查找对应 handler 并传入 params。
fn dispatch_notification(
    line: &str,
    handlers: &Arc<Mutex<NotificationHandlerMap>>,
) {
    let notification: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(method) = notification.get("method").and_then(|v| v.as_str()) {
        let handlers = handlers.lock().unwrap();
        if let Some(handler) = handlers.get(method) {
            if let Some(params) = notification.get("params") {
                handler(params.clone());
            }
        }
    }
}
