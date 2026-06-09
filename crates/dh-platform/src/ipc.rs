use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpcError {
    #[error("Platform not supported")]
    Unsupported,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Filesystem error: {0}")]
    Fs(#[from] super::fs::FsError),
}

pub enum IpcEndpoint {
    #[cfg(unix)]
    UnixSocket(PathBuf),
    #[cfg(windows)]
    NamedPipe(String),
}

impl IpcEndpoint {
    pub fn default_gatewayd() -> Result<Self, IpcError> {
        #[cfg(unix)]
        {
            let dir = super::fs::data_dir()?;
            Ok(IpcEndpoint::UnixSocket(dir.join("gatewayd.sock")))
        }

        #[cfg(windows)]
        {
            Ok(IpcEndpoint::NamedPipe(r"\\.\pipe\deepharness-gatewayd".to_string()))
        }
    }
}

#[cfg(unix)]
pub mod unix {
    use tokio::net::UnixListener;

    pub async fn bind_socket(path: &std::path::Path) -> Result<UnixListener, super::IpcError> {
        let _ = tokio::fs::remove_file(path).await;
        Ok(UnixListener::bind(path)?)
    }
}
