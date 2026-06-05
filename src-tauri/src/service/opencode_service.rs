use std::process::{Child, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct OpencodeService {
    serve_process: Arc<Mutex<Option<Child>>>,
    port: u16,
    base_url: String,
    client: reqwest::Client,
}

impl OpencodeService {
    pub fn new() -> Result<Self, String> {
        let port = Self::find_available_port_sync()?;

        let mut cmd = std::process::Command::new("opencode");
        cmd.arg("serve")
            .arg("--port")
            .arg(port.to_string())
            .arg("--pure")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start opencode serve: {}", e))?;

        std::thread::sleep(std::time::Duration::from_secs(3));

        let base_url = format!("http://127.0.0.1:{}", port);

        Ok(Self {
            serve_process: Arc::new(Mutex::new(Some(child))),
            port,
            base_url,
            client: reqwest::Client::new(),
        })
    }

    pub fn new_fallback() -> Self {
        let port = 3001;
        let base_url = format!("http://127.0.0.1:{}", port);
        Self {
            serve_process: Arc::new(Mutex::new(None)),
            port,
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn get_attach_url(&self) -> String {
        self.base_url.clone()
    }

    /// 通过 opencode serve HTTP API 创建新会话
    pub async fn create_session(&self) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .post(format!("{}/session", self.base_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| format!("Failed to create session: {}", e))?;

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse create_session response: {}", e))
    }

    /// 通过 opencode serve HTTP API 发送消息
    pub async fn send_message(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .post(format!("{}/session/{}/message", self.base_url, session_id))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "parts": [{ "type": "text", "text": message }]
            }))
            .send()
            .await
            .map_err(|e| format!("Failed to send message: {}", e))?;

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse send_message response: {}", e))
    }

    /// 执行消息并返回结果
    ///
    /// 流程：
    /// 1. 如果没有 session_id，先调用 create_session 创建
    /// 2. 调用 send_message 发送消息
    /// 3. 解析返回的 parts 提取文本内容
    pub async fn run_message(
        &self,
        message: &str,
        session_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        // 创建或复用 session
        let sid = match session_id {
            Some(sid) if !sid.is_empty() => sid.to_string(),
            _ => {
                let session = self.create_session().await?;
                session
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        "Failed to get session ID from create_session response".to_string()
                    })?
            }
        };

        let result = self.send_message(&sid, message).await?;

        let session_id_result = result
            .get("info")
            .and_then(|i| i.get("sessionID"))
            .and_then(|v| v.as_str())
            .unwrap_or(&sid)
            .to_string();

        let mut text_parts: Vec<String> = Vec::new();

        if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
        }

        if text_parts.is_empty() {
            text_parts.push("opencode 未返回内容".to_string());
        }

        Ok(serde_json::json!({
            "sessionID": session_id_result,
            "parts": text_parts.iter().map(|t| {
                serde_json::json!({
                    "type": "text",
                    "text": t
                })
            }).collect::<Vec<_>>(),
        }))
    }

    fn find_available_port_sync() -> Result<u16, String> {
        for port in 3001..=3010 {
            if Self::is_port_available_sync(port) {
                return Ok(port);
            }
        }
        Err("No available port found in range 3001-3010".to_string())
    }

    fn is_port_available_sync(port: u16) -> bool {
        std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
    }
}

impl Drop for OpencodeService {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.serve_process.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
            }
        }
    }
}
