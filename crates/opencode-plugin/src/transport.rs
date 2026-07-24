use agent_core::error::InstanceError;
use agent_core::process::http::HttpTransport;
use agent_core::process::transport::TransportHandle;
use serde_json::json;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;

const OPCODE_BINARY: &str = "opencode";
const ARG_SERVE: &str = "serve";
const ARG_PORT: &str = "--port";

/// 消息 POST 会阻塞至 agent run 结束，PRD/原型生成等正常 run 可超过 10 分钟，
/// 超时过短会中途取消 opencode 的 run 并触发重启重试（重复消耗 LLM 调用）。
///
/// 注意：真正的卡死检测由 instance 层的 SSE 活跃度看门狗负责
///（`WATCHDOG_STALL_THRESHOLD_SECS`，无事件即重建重试）；本超时仅作为最终硬上限
/// 兜底，防止 HTTP 永久挂起。两者配合：正常长任务由看门狗放行（有事件不触发），
/// 异常挂起由看门狗在阈值内捕获，本超时处理看门狗无法覆盖的极端场景。
const DEFAULT_TIMEOUT_SECS: u64 = 1200;
/// 健康检查的单次请求超时：挂起的探测必须快速失败，
/// 否则启动重试循环（20 次 × 500ms）会被一次卡死的探测拖住。
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 2;
const HEALTH_PATH: &str = "/health";
const SESSION_PATH: &str = "/session";
const MESSAGE_PATH_SUFFIX: &str = "/message";
const CONTENT_TYPE_JSON: &str = "application/json";
const HEADER_CONTENT_TYPE: &str = "Content-Type";

const KEY_ID: &str = "id";
const KEY_PARTS: &str = "parts";
const KEY_TYPE: &str = "type";
const KEY_TEXT: &str = "text";
const BODY_TYPE_TEXT: &str = "text";
const ERR_MISSING_SESSION_ID: &str = "Missing session id";

const LOCALHOST_BIND_PREFIX: &str = "127.0.0.1:";

const ERR_START_OPCODE_SERVE_PREFIX: &str = "Failed to start opencode serve: ";
const ERR_CREATE_SESSION_PREFIX: &str = "create_session: ";
const ERR_SEND_MESSAGE_PREFIX: &str = "send_message: ";
const ERR_NO_AVAILABLE_PORT_PREFIX: &str = "No available port found in range ";
const ERR_SSE_CONNECT_PREFIX: &str = "SSE connect failed: ";

const PORT_RANGE_START: u16 = 3001;
const PORT_RANGE_END: u16 = 3050;

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base, path)
}

fn prefixed<E: std::fmt::Display>(prefix: &str, value: E) -> String {
    format!("{}{}", prefix, value)
}

/// HTTP client for the OpenCode `serve` endpoints.
pub struct OpenCodeClient {
    client: reqwest::Client,
    base_url: String,
}

impl OpenCodeClient {
    /// Creates a client that talks to `base_url`.
    pub fn new(base_url: impl Into<String>) -> Self {
        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// Returns the underlying HTTP client.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Performs a health check against `/health`.
    pub async fn health_check(&self) -> bool {
        let url = join_url(&self.base_url, HEALTH_PATH);
        self.client
            .get(&url)
            .timeout(Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
            .send()
            .await
            .is_ok()
    }

    /// Creates a new OpenCode session and returns its id.
    pub async fn create_session(&self) -> Result<String, InstanceError> {
        let url = join_url(&self.base_url, SESSION_PATH);
        let resp = self
            .client
            .post(&url)
            .header(HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON)
            .json(&json!({}))
            .send()
            .await
            .map_err(|e| InstanceError::SendFailed(prefixed(ERR_CREATE_SESSION_PREFIX, e)))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("parse {e}")))?;

        body.get(KEY_ID)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| InstanceError::SendFailed(ERR_MISSING_SESSION_ID.into()))
    }

    /// Sends `message` to the given OpenCode `session_id`.
    pub async fn send_message(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        let url = format!(
            "{}{}/{}{}",
            self.base_url, SESSION_PATH, session_id, MESSAGE_PATH_SUFFIX
        );
        let resp = self
            .client
            .post(&url)
            .header(HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON)
            .json(&json!({
                KEY_PARTS: [{ KEY_TYPE: BODY_TYPE_TEXT, KEY_TEXT: message }]
            }))
            .send()
            .await
            .map_err(|e| InstanceError::SendFailed(prefixed(ERR_SEND_MESSAGE_PREFIX, e)))?;

        resp.json()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("parse {e}")))
    }
}

/// Spawns `opencode serve` on the given port, rooted at `work_directory`.
///
/// 显式设置子进程 CWD 为目标工作区，避免继承 gatewayd 自身目录。
/// 若不设置，opencode serve 会以 gatewayd 的 CWD 为工作区，导致
/// 文件操作落到错误目录（与 claude-plugin 的 StdioTransport 行为对齐）。
///
/// 对 `work_directory` 做规范化并检查存在性，避免路径遍历或指向不存在的目录。
pub fn start_opencode_process(port: u16, work_directory: &str) -> Result<Child, InstanceError> {
    let cwd = std::fs::canonicalize(work_directory).map_err(|e| {
        InstanceError::ProcessError(format!(
            "invalid work_directory '{}': {}",
            work_directory, e
        ))
    })?;
    if !cwd.is_dir() {
        return Err(InstanceError::ProcessError(format!(
            "work_directory '{}' is not a directory",
            cwd.display()
        )));
    }

    let mut cmd = tokio::process::Command::new(OPCODE_BINARY);
    cmd.arg(ARG_SERVE)
        .arg(ARG_PORT)
        .arg(port.to_string())
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| InstanceError::ProcessError(prefixed(ERR_START_OPCODE_SERVE_PREFIX, e)))?;

    // 子进程 stdout/stderr 为 piped 但无人消费时，管道缓冲区（约 64KB）写满后
    // 子进程会阻塞在 write() 上冻结（端口仍在监听，表现为假死）。
    // 这里接管管道并持续读取，仅记录日志。
    drain_child_pipes(&mut child, port);

    Ok(child)
}

/// 子进程输出管道类型，用于区分日志级别。
#[derive(Debug, Clone, Copy)]
enum PipeKind {
    Stdout,
    Stderr,
}

/// 接管子进程的 stdout/stderr 并各启动一个后台任务持续排空。
/// 注意：tokio::spawn 要求处于 runtime 上下文中，
/// 当前所有调用方（start_opencode_with_retry）均在 async 环境内调用本函数。
fn drain_child_pipes(child: &mut Child, port: u16) {
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(drain_pipe(stdout, PipeKind::Stdout, port));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(drain_pipe(stderr, PipeKind::Stderr, port));
    }
}

/// 按行读取管道内容并记录日志，直到 EOF 或读取出错。
async fn drain_pipe<R>(reader: R, kind: PipeKind, port: u16)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => log_pipe_line(kind, port, &line),
            Ok(None) => break,
            Err(e) => {
                log::warn!("opencode[port={}] {:?} read error: {}", port, kind, e);
                break;
            }
        }
    }
}

/// stdout 记 debug（正常输出），stderr 记 warn（便于排查子进程异常）。
fn log_pipe_line(kind: PipeKind, port: u16, line: &str) {
    match kind {
        PipeKind::Stdout => log::debug!("opencode[port={}] stdout: {}", port, line),
        PipeKind::Stderr => log::warn!("opencode[port={}] stderr: {}", port, line),
    }
}

/// Thread-safe allocator for OpenCode ports in the configured range.
///
/// Instead of the previous "bind-and-release" check which has a TOCTOU race,
/// this allocator hands out a monotonically increasing port (wrapping around
/// at the end of the range).  If the chosen port is actually occupied, the
/// caller retries with the next allocation.  This eliminates the window where
/// another process could grab the port between check and use.
pub struct PortAllocator {
    next: AtomicU16,
    start: u16,
    end: u16,
}

impl PortAllocator {
    pub fn new(start: u16, end: u16) -> Self {
        Self {
            next: AtomicU16::new(start),
            start,
            end,
        }
    }

    /// Returns the next candidate port in the range.  The caller is responsible
    /// for verifying the port is actually usable (e.g. by spawning the child
    /// and checking health).  If it is not, allocate again and retry.
    pub fn allocate(&self) -> u16 {
        loop {
            let current = self.next.load(Ordering::Relaxed);
            let next_port = if current >= self.end {
                self.start
            } else {
                current + 1
            };
            match self.next.compare_exchange(
                current,
                next_port,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return current,
                Err(_) => continue,
            }
        }
    }
}

/// Global OpenCode port allocator.  The range is shared across all
/// OpencodeInstance instances in the process.
static PORT_ALLOCATOR: OnceLock<PortAllocator> = OnceLock::new();

pub fn port_allocator() -> &'static PortAllocator {
    PORT_ALLOCATOR.get_or_init(|| PortAllocator::new(PORT_RANGE_START, PORT_RANGE_END))
}

/// Finds an available TCP port in the default OpenCode range.
///
/// Deprecated: prefer `port_allocator().allocate()` + retry on bind failure.
pub fn find_available_port() -> Result<u16, String> {
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if std::net::TcpListener::bind(join_url(LOCALHOST_BIND_PREFIX, &port.to_string())).is_ok() {
            return Ok(port);
        }
    }
    Err(format!(
        "{}{}-{}",
        ERR_NO_AVAILABLE_PORT_PREFIX, PORT_RANGE_START, PORT_RANGE_END
    ))
}

/// Connects the SSE stream for an OpenCode instance.
pub async fn connect_opencode_sse(
    base_url: &str,
    client: reqwest::Client,
    instance_id: &str,
    sender: mpsc::Sender<serde_json::Value>,
) -> Result<Box<dyn TransportHandle>, InstanceError> {
    HttpTransport::with_client(base_url, client)
        .connect_sse(instance_id.to_string(), sender)
        .await
        .map_err(|e| InstanceError::ProcessError(prefixed(ERR_SSE_CONNECT_PREFIX, e)))
}
