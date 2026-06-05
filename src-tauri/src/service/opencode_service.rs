use std::process::{Child, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct OpencodeService {
    serve_process: Arc<Mutex<Option<Child>>>,
    port: u16,
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

        let child = cmd.spawn()
            .map_err(|e| format!("Failed to start opencode serve: {}", e))?;

        std::thread::sleep(std::time::Duration::from_secs(3));

        Ok(Self {
            serve_process: Arc::new(Mutex::new(Some(child))),
            port,
        })
    }

    pub fn new_fallback() -> Self {
        Self {
            serve_process: Arc::new(Mutex::new(None)),
            port: 3001,
        }
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn get_attach_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// 执行 opencode run 并实时推送事件
    /// 
    /// callback 会在每读取一行 JSON 时被调用
    pub async fn run_message(
        &self,
        message: &str,
        session_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let mut cmd = tokio::process::Command::new("opencode");
        cmd.arg("run")
            .arg(message)
            .arg("--format")
            .arg("json");

        if let Some(sid) = session_id {
            cmd.arg("--session").arg(sid);
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| format!("Failed to execute opencode run: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // 解析 JSON Lines 输出
        let mut session_id_result = String::new();
        let mut text_parts: Vec<String> = Vec::new();

        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                if session_id_result.is_empty() {
                    if let Some(sid) = event.get("sessionID").or_else(|| event.get("sessionId")).or_else(|| event.get("session_id")).and_then(|v| v.as_str()) {
                        session_id_result = sid.to_string();
                    }
                }

                if let Some(text) = event.get("content").or_else(|| event.get("text")).and_then(|v| v.as_str()) {
                    text_parts.push(text.to_string());
                }

                if let Some(part) = event.get("part") {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }

                if let Some(parts) = event.get("parts").and_then(|v| v.as_array()) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                }
            }
        }

        if !output.status.success() {
            return Err(format!("opencode run failed: {}", stderr));
        }

        if text_parts.is_empty() {
            text_parts.push(if stderr.trim().is_empty() {
                "opencode 未返回内容".to_string()
            } else {
                stderr.trim().to_string()
            });
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
