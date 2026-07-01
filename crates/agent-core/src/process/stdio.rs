//! Stdio transport using NDJSON framing.
//!
//! Messages are exchanged as single-line JSON objects terminated by a newline
//! (`\n`). This mirrors the standard MCP (Model Context Protocol) stdio
//! transport and keeps parsing trivial: read one line, parse it as JSON, and
//! write a JSON payload followed by a newline.

use crate::process::transport::{Transport, TransportError, TransportHandle};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const MESSAGE_DELIMITER: &[u8] = b"\n";
const ERR_STDIN_UNAVAILABLE: &str = "stdin unavailable";
const ERR_STDOUT_UNAVAILABLE: &str = "stdout unavailable";
const CLOSE_TIMEOUT_SECS: u64 = 5;
const STDERR_LOG_PREFIX: &str = "agent stderr";

pub struct StdioTransport {
    program: String,
    args: Vec<String>,
    cwd: String,
}

impl StdioTransport {
    pub fn new(program: impl Into<String>, args: Vec<String>, cwd: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: cwd.into(),
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| TransportError::ProcessStart(format!("{}: {}", self.program, e)))?;

        // Take ownership of the child's stdio pipes. If a pipe is missing the
        // process was spawned with an unexpected configuration.
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| TransportError::ProcessStart(ERR_STDIN_UNAVAILABLE.into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TransportError::ProcessStart(ERR_STDOUT_UNAVAILABLE.into()))?;

        // Drain stderr in the background so the child never blocks because its
        // stderr pipe is full. Each line is logged for debugging long-running
        // agents; the task exits automatically when the pipe closes.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("{STDERR_LOG_PREFIX}: {line}");
                }
            });
        }

        Ok(Box::new(StdioHandle {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout).lines(),
        }))
    }

    fn endpoint(&self) -> Option<String> {
        None
    }
}

struct StdioHandle {
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: tokio::io::Lines<BufReader<ChildStdout>>,
}

impl StdioHandle {
    /// Convert any send-time I/O error into a [`TransportError::SendFailed`].
    fn map_send_err<E: std::fmt::Display>(e: E) -> TransportError {
        TransportError::SendFailed(e.to_string())
    }

    /// Write all bytes to the underlying writer, mapping errors to
    /// [`TransportError::SendFailed`].
    async fn write_all_bytes(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.writer.write_all(bytes).await.map_err(Self::map_send_err)
    }

    /// Write a complete NDJSON line: payload, delimiter, and flush.
    async fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        self.write_all_bytes(line.as_bytes()).await?;
        self.write_all_bytes(MESSAGE_DELIMITER).await?;
        self.writer.flush().await.map_err(Self::map_send_err)
    }
}

#[async_trait]
impl TransportHandle for StdioHandle {
    async fn send(&mut self, payload: Value) -> Result<(), TransportError> {
        // Serialize the JSON payload and write it as a single NDJSON line.
        let line = serde_json::to_string(&payload).map_err(Self::map_send_err)?;
        self.write_line(&line).await
    }

    async fn receive(&mut self) -> Result<Value, TransportError> {
        // Read the next NDJSON line from stdout and parse it as JSON.
        match self.reader.next_line().await {
            Ok(Some(line)) => serde_json::from_str(&line)
                .map_err(|e| TransportError::ReceiveFailed(format!("{e}: {line}"))),
            Ok(None) => Err(TransportError::Closed),
            Err(e) => Err(TransportError::ReceiveFailed(e.to_string())),
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Request the child process to terminate, then wait briefly for it to
        // exit so we don't leave zombie processes behind.
        let _ = self.child.start_kill().ok();
        match tokio::time::timeout(Duration::from_secs(CLOSE_TIMEOUT_SECS), self.child.wait()).await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(TransportError::ProcessExit(e.to_string())),
            Err(_) => {
                // Timeout: the process did not exit in time and may become a
                // zombie. Return Ok so callers can continue cleanup.
                Ok(())
            }
        }
    }

    fn is_alive(&mut self) -> bool {
        // try_wait returns Some if the process has already exited.
        // None means it's still running (or we haven't waited yet).
        self.child.try_wait().ok().flatten().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stdio_echo() {
        let transport = StdioTransport::new("cat".to_string(), vec![], ".".to_string());
        let mut handle = transport.start().await.unwrap();
        handle.send(serde_json::json!({"hello":"world"})).await.unwrap();
        let value = handle.receive().await.unwrap();
        assert_eq!(value["hello"], "world");
        handle.close().await.unwrap();
    }
}
