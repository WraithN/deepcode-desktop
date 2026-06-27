//! Error types for the dh-config crate.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse TOML at {path}: {source}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to serialize TOML: {0}")]
    SerializeToml(#[from] toml::ser::Error),

    #[error("placeholder '{placeholder}' could not be resolved: {reason}")]
    Placeholder {
        placeholder: String,
        reason: String,
    },

    #[error("placeholder recursion exceeded maximum depth ({0})")]
    PlaceholderDepthExceeded(usize),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("unknown placeholder kind: {0}")]
    UnknownPlaceholderKind(String),

    #[error("home directory could not be determined")]
    NoHomeDir,

    #[error("config dir could not be determined")]
    NoConfigDir,
}

pub type Result<T> = std::result::Result<T, ConfigError>;
