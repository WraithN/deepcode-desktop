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
