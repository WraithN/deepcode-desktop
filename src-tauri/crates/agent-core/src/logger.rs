use serde::{Serialize, Deserialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use rusqlite::params;

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub fn new(app_handle: AppHandle, db_conn: rusqlite::Connection) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<SessionLogEntry>();

        std::thread::spawn(move || {
            while let Some(entry) = rx.blocking_recv() {
                let _ = app_handle.emit("session:log", &entry);
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
    ) {
        let entry = SessionLogEntry {
            conversation_id: conversation_id.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            source: source.to_string(),
            message: message.to_string(),
            payload,
        };
        let _ = self.sender.send(entry);
    }
}
