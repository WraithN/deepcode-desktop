//! Error types for adapter operations.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("serialization failed: {0}")]
    Serialize(String),

    #[error("config layer error: {0}")]
    Config(#[from] dh_config::ConfigError),

    #[error("adapter `{adapter}` does not support scope: {scope}")]
    UnsupportedScope { adapter: String, scope: String },

    #[error("backup not found: {0}")]
    BackupNotFound(String),

    #[error("render produced too many files ({0})")]
    TooManyFiles(usize),

    #[error("unsupported feature: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, AdapterError>;

impl From<serde_json::Error> for AdapterError {
    fn from(err: serde_json::Error) -> Self {
        AdapterError::Serialize(err.to_string())
    }
}
