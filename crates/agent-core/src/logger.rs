use crate::event_sink::DynEventSink;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use tokio::sync::mpsc;
use rusqlite::params;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionLogEntry {
    pub conversation_id: String,
    pub instance_id: Option<String>,
    pub timestamp: String,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
    pub payload: Option<Value>,
}

#[derive(Clone)]
pub struct SessionLogger {
    sender: mpsc::UnboundedSender<SessionLogEntry>,
}

impl SessionLogger {
    /// Create a new logger.
    ///
    /// * `sink`         – event sink for broadcasting `session:log` events.
    /// * `db_conn`      – SQLite connection (moved into a background thread).
    /// * `log_file`     – optional path to a local log file.
    pub fn new(
        sink: DynEventSink,
        db_conn: rusqlite::Connection,
        log_file: Option<PathBuf>,
    ) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<SessionLogEntry>();

        let mut log_writer = log_file.and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
                .map(|f| std::io::LineWriter::new(f))
        });

        let _ = db_conn.execute(
            "CREATE TABLE IF NOT EXISTS session_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                instance_id TEXT,
                timestamp TEXT NOT NULL,
                level TEXT NOT NULL,
                source TEXT NOT NULL,
                message TEXT NOT NULL,
                payload TEXT
            )",
            [],
        );

        std::thread::spawn(move || {
            while let Some(entry) = rx.blocking_recv() {
                // Emit via EventSink (WebSocket, HTTP SSE, etc.)
                let _ = sink.emit("session:log", serde_json::to_value(&entry).unwrap_or_default());

                // Persist to SQLite
                let _ = db_conn.execute(
                    "INSERT INTO session_logs (conversation_id, timestamp, level, source, message, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        &entry.conversation_id,
                        &entry.timestamp,
                        entry.level.as_str(),
                        &entry.source,
                        &entry.message,
                        entry.payload.as_ref().map(|v| v.to_string())
                    ],
                );

                // Append to local log file
                if let Some(ref mut writer) = log_writer {
                    let payload_str = entry.payload.as_ref().map(|v| v.to_string()).unwrap_or_default();
                    let line = if payload_str.is_empty() || payload_str == "null" {
                        format!(
                            "[{}] [{}] [{}] {} - {}\n",
                            entry.timestamp,
                            entry.level.as_str().to_uppercase(),
                            entry.source,
                            entry.conversation_id,
                            entry.message
                        )
                    } else {
                        format!(
                            "[{}] [{}] [{}] {} - {} | {}\n",
                            entry.timestamp,
                            entry.level.as_str().to_uppercase(),
                            entry.source,
                            entry.conversation_id,
                            entry.message,
                            payload_str
                        )
                    };
                    let _ = std::io::Write::write_all(writer, line.as_bytes());
                    let _ = std::io::Write::flush(writer);
                }
            }
        });

        Self { sender: tx }
    }

    pub fn log(
        &self,
        conversation_id: &str,
        level: LogLevel,
        source: &str,
        message: &str,
        payload: Option<Value>,
        instance_id: Option<String>,
    ) {
        let entry = SessionLogEntry {
            conversation_id: conversation_id.to_string(),
            instance_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            source: source.to_string(),
            message: message.to_string(),
            payload,
        };
        let _ = self.sender.send(entry);
    }

    /// Simplified log without instance_id.
    pub fn log_simple(
        &self,
        conversation_id: &str,
        level: LogLevel,
        source: &str,
        message: &str,
        payload: Option<Value>,
    ) {
        self.log(conversation_id, level, source, message, payload, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }

    #[test]
    fn test_log_level_equality() {
        assert_eq!(LogLevel::Info, LogLevel::Info);
        assert_ne!(LogLevel::Info, LogLevel::Error);
    }
}
