use super::types::McpError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex as TokioMutex};

/// 请求等待超时时间（秒）
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// 优雅关闭子进程的超时时间（秒）
const SHUTDOWN_TIMEOUT_SECS: u64 = 5;
/// JSON 内容类型
const CONTENT_TYPE_JSON: &str = "application/json";
/// SSE `data:` 行前缀
const SSE_DATA_PREFIX: &str = "data: ";
/// SSE 内容类型（用于响应 Content-Type 匹配）
const TEXT_EVENT_STREAM: &str = "text/event-stream";
/// Accept 头部值：优先 JSON，兼容 SSE
const ACCEPT_VALUE: &str = "application/json, text/event-stream";
/// HTTP 错误前缀
const HTTP_ERROR_PREFIX: &str = "HTTP ";
/// SSE 响应缺少 data 行的错误消息
const SSE_NO_DATA_LINE: &str = "SSE response without data line";

/// MCP 传输层抽象。
///
/// 上层（`McpClient`）通过该 trait 与 MCP server 通信：
/// - [`McpTransport::send_request`] 发送 JSON-RPC 请求并同步等待响应字符串
/// - [`McpTransport::set_notification_handler`] 注册无 id 消息的回调
///
/// `StdioTransport`（子进程 stdio）与 `HttpTransport`（Task 4，HTTP SSE）分别实现该 trait。
/// 该抽象把「按 JSON-RPC id 路由响应」的逻辑下沉到 transport 层，
/// 使上层无需关心底层传输细节。
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// 发送 JSON-RPC 请求并同步等待响应。
    ///
    /// 入参为序列化后的 JSON-RPC 请求字符串；返回值为响应的 JSON 字符串。
    /// transport 内部维护 `id -> oneshot::Sender` 的 pending 表，
    /// 在 stdout reader 任务中按 id 路由响应。
    async fn send_request(&self, json: String) -> Result<String, McpError>;

    /// 检查底层连接/进程是否存活。
    async fn is_alive(&self) -> bool;

    /// 关闭传输层（终止子进程 / 关闭连接）。
    async fn close(&self) -> Result<(), McpError>;

    /// 注册 notification 处理器。
    ///
    /// transport 在收到无 id 的 JSON-RPC 消息（如 `tools/list_changed`）时，
    /// 调用此 handler 并传入原始 JSON 字符串。
    fn set_notification_handler(&self, handler: Box<dyn Fn(String) + Send>);
}

/// notification 处理器类型别名
type NotificationHandler = Box<dyn Fn(String) + Send>;

/// 基于 stdio 的 MCP 传输层实现。
///
/// 通过子进程的 stdin/stdout 与 MCP server 通信。
/// 内部维护：
/// - `pending`：JSON-RPC id -> oneshot sender 表，用于 send_request 同步等待响应
/// - `notification_handler`：无 id 消息的回调
/// - stdout reader 任务：按 id 路由响应到 pending，无 id 的送入 notification_handler
pub struct StdioTransport {
    /// 子进程 stdin（并发写需要互斥）
    stdin: TokioMutex<ChildStdin>,
    /// 子进程（start_kill / wait 需要 &mut，故加锁）
    child: TokioMutex<Child>,
    /// pending 请求表：JSON-RPC id -> oneshot sender
    pending: Arc<TokioMutex<HashMap<u64, oneshot::Sender<String>>>>,
    /// notification 处理器（无 id 消息回调）
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    /// 自增 request id（仅当请求 JSON 缺失 id 时兜底生成）
    next_id: AtomicU64,
}

impl StdioTransport {
    /// 启动 MCP server 子进程并建立传输层。
    ///
    /// 返回的 `StdioTransport` 内部已 spawn stdout/stderr reader 任务：
    /// - stdout 按 JSON-RPC id 路由响应到 pending 表
    /// - 无 id 的消息送入 notification_handler
    /// - stderr 仅记录日志
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        workspace: &str,
    ) -> Result<Self, McpError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, val) in env {
            cmd.env(key, val);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::ProcessError(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::ProcessError("Failed to open stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::ProcessError("Failed to open stdout".to_string()))?;

        let pending: Arc<TokioMutex<HashMap<u64, oneshot::Sender<String>>>> =
            Arc::new(TokioMutex::new(HashMap::new()));
        let notification_handler: Arc<Mutex<Option<NotificationHandler>>> =
            Arc::new(Mutex::new(None));

        let pending_clone = pending.clone();
        let handler_clone = notification_handler.clone();

        // stdout reader 任务：按 JSON-RPC id 路由响应到 pending，
        // 无 id 的消息交给 notification_handler。
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                route_stdout_line(&line, &pending_clone, &handler_clone).await;
            }
        });

        // stderr reader 任务：仅记录日志
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::warn!("MCP stderr: {}", line);
                }
            });
        }

        Ok(Self {
            stdin: TokioMutex::new(stdin),
            child: TokioMutex::new(child),
            pending,
            notification_handler,
            next_id: AtomicU64::new(1),
        })
    }

    /// 发送原始 JSON 行（用于 notification 等无需响应的场景）。
    ///
    /// 仅写入 stdin 并 flush，不等待响应。
    pub async fn send(&self, message: String) -> Result<(), McpError> {
        let mut stdin = self.stdin.lock().await;
        let json = format!("{}\n", message);
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| McpError::ProcessError(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| McpError::ProcessError(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send_request(&self, json: String) -> Result<String, McpError> {
        // 确保 JSON-RPC 请求携带 u64 id：已有则沿用，无则用自增 id 写回 JSON
        let (id, json_to_send) = ensure_request_id(&json, &self.next_id)?;

        // 登记 pending oneshot，等待 stdout reader 按 id 路由响应
        let (tx, rx) = oneshot::channel::<String>();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // 写入 stdin 发出请求
        self.send(json_to_send).await?;

        // 等待响应（带超时）
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(McpError::ProtocolError("Request cancelled".to_string())),
            Err(_) => {
                // 超时：清理 pending 表，避免 sender 泄漏
                self.pending.lock().await.remove(&id);
                Err(McpError::RequestTimeout)
            }
        }
    }

    async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => false,
        }
    }

    async fn close(&self) -> Result<(), McpError> {
        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(SHUTDOWN_TIMEOUT_SECS),
            child.wait(),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(McpError::ProcessError(e.to_string())),
            Err(_) => Err(McpError::ProcessError(
                "MCP child process did not exit within shutdown timeout".to_string(),
            )),
        }
    }

    fn set_notification_handler(&self, handler: Box<dyn Fn(String) + Send>) {
        let mut guard = self.notification_handler.lock().unwrap();
        *guard = Some(handler);
    }
}

/// 解析 JSON-RPC 请求中的 id（u64）。
///
/// 支持数值 id 和可解析为 u64 的字符串 id。
/// 若无 id 或解析失败则返回 `None`。
fn parse_request_id(json: &str) -> Result<Option<u64>, McpError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| McpError::ProtocolError(e.to_string()))?;
    match value.get("id") {
        Some(Value::Number(n)) => Ok(n.as_u64()),
        Some(Value::String(s)) => Ok(s.parse::<u64>().ok()),
        _ => Ok(None),
    }
}

/// 确保 JSON-RPC 请求携带 u64 id。
///
/// 若原 JSON 已有可解析的 u64 id 则沿用；否则用 `next_id` 分配并写回 JSON。
/// 返回 `(id, 最终发送的 JSON 字符串)`。
fn ensure_request_id(
    json: &str,
    next_id: &AtomicU64,
) -> Result<(u64, String), McpError> {
    match parse_request_id(json)? {
        Some(id) => Ok((id, json.to_string())),
        None => {
            // 请求缺失 id：分配自增 id 并写回 JSON
            let id = next_id.fetch_add(1, Ordering::SeqCst);
            let mut value: Value = serde_json::from_str(json)
                .map_err(|e| McpError::ProtocolError(e.to_string()))?;
            value["id"] = Value::from(id);
            let serialized = serde_json::to_string(&value)
                .map_err(|e| McpError::ProtocolError(e.to_string()))?;
            Ok((id, serialized))
        }
    }
}

/// 路由 stdout 单行：有 id 的响应送入 pending 表，无 id 的送入 notification_handler。
///
/// 该函数由 stdout reader 任务对每一行调用：
/// 1. 尝试解析为 JSON；解析失败则记录日志并丢弃（避免污染 pending 表）
/// 2. 若包含 u64 id，从 pending 表取出对应 oneshot sender 并发送响应
/// 3. 若无 id（notification），调用注册的 notification_handler
async fn route_stdout_line(
    line: &str,
    pending: &Arc<TokioMutex<HashMap<u64, oneshot::Sender<String>>>>,
    notification_handler: &Arc<Mutex<Option<NotificationHandler>>>,
) {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            log::warn!("MCP stdout non-JSON line: {}", line);
            return;
        }
    };
    match value.get("id").and_then(|v| v.as_u64()) {
        Some(id) => {
            if let Some(sender) = pending.lock().await.remove(&id) {
                let _ = sender.send(line.to_string());
            }
        }
        None => {
            // 无 id 的消息为 notification
            let guard = notification_handler.lock().unwrap();
            if let Some(handler) = guard.as_ref() {
                handler(line.to_string());
            }
        }
    }
}

/// HTTP 传输层实现（MCP Streamable HTTP）。
///
/// 通过 POST JSON-RPC 请求到 MCP server URL 与之通信。支持两种响应格式：
/// - `application/json`：直接返回 body
/// - `text/event-stream`（SSE）：取首个 `data:` 行的 payload
///
/// 用于连接 crawler-service 的 `/mcp` 端点（Task 2）。
pub struct HttpTransport {
    client: reqwest::Client,
    url: String,
}

impl HttpTransport {
    /// 创建 HTTP 传输层。
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send_request(&self, json: String) -> Result<String, McpError> {
        let resp = self
            .client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE_JSON)
            .header(reqwest::header::ACCEPT, ACCEPT_VALUE)
            .body(json)
            .send()
            .await
            .map_err(|e| McpError::ProcessError(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(McpError::ProcessError(format!(
                "{HTTP_ERROR_PREFIX}{}",
                resp.status()
            )));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = resp
            .text()
            .await
            .map_err(|e| McpError::ProcessError(e.to_string()))?;
        // SSE 响应：取首个 data: 行的 payload；JSON 响应：直接返回 body。
        if content_type.contains(TEXT_EVENT_STREAM) {
            return extract_sse_payload(&body)
                .ok_or_else(|| McpError::ProtocolError(SSE_NO_DATA_LINE.to_string()));
        }
        Ok(body)
    }

    async fn is_alive(&self) -> bool {
        true
    }

    async fn close(&self) -> Result<(), McpError> {
        Ok(())
    }

    fn set_notification_handler(&self, _handler: Box<dyn Fn(String) + Send>) {
        // HTTP 传输层走请求-响应模型，暂不处理无 id 的 notification。
    }
}

/// 从 SSE 响应 body 中提取首个 `data:` 行的 payload。
///
/// SSE 格式：每行形如 `field: value`，其中 `data:` 字段携带 JSON-RPC 响应。
/// 返回首个 `data:` 行的内容（已去掉 `data: ` 前缀）。
fn extract_sse_payload(body: &str) -> Option<String> {
    for line in body.lines() {
        if let Some(payload) = line.strip_prefix(SSE_DATA_PREFIX) {
            return Some(payload.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_transport_implements_trait() {
        // 编译期断言：StdioTransport 满足 McpTransport trait（无需构造实例）。
        fn _assert<T: McpTransport>() {}
        _assert::<StdioTransport>();
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use mockito::Server;

    #[tokio::test]
    async fn http_transport_posts_jsonrpc_and_returns_response() {
        let mut server = Server::new_async().await;
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let resp = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let m = server
            .mock("POST", "/mcp")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(resp)
            .create_async()
            .await;
        let t = HttpTransport::new(format!("{}/mcp", server.url()));
        let out = t.send_request(body.to_string()).await.unwrap();
        assert!(out.contains(r#""tools":[]"#));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn http_transport_returns_err_on_5xx() {
        let mut server = Server::new_async().await;
        // 绑定到 _m 以保证 mock 在请求期间存活；mockito guard drop 时会移除该 mock。
        let _m = server
            .mock("POST", "/mcp")
            .with_status(503)
            .create_async()
            .await;
        let t = HttpTransport::new(format!("{}/mcp", server.url()));
        let r = t
            .send_request(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string(),
            )
            .await;
        assert!(r.is_err());
    }
}
