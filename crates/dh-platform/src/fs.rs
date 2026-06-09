use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FsError {
    #[error("Failed to get data directory: {0}")]
    DataDir(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn data_dir() -> Result<PathBuf, FsError> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or_else(|| FsError::DataDir("No home dir".into()))?;
        Ok(home.join("Library/Application Support/DeepHarness"))
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or_else(|| FsError::DataDir("No home dir".into()))?;
        Ok(home.join(".local/share/deepharness"))
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data = dirs::data_local_dir()
            .ok_or_else(|| FsError::DataDir("No local app data dir".into()))?;
        Ok(local_app_data.join("DeepHarness"))
    }
}

pub fn ensure_data_dir() -> Result<PathBuf, FsError> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn lock_file_path() -> Result<PathBuf, FsError> {
    Ok(data_dir()?.join("gatewayd.lock"))
}

pub fn write_lock_file(pid: u32) -> Result<(), FsError> {
    let path = lock_file_path()?;
    std::fs::write(&path, pid.to_string())?;
    Ok(())
}

pub fn read_lock_file() -> Result<Option<u32>, FsError> {
    let path = lock_file_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content.trim().parse().ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn remove_lock_file() -> Result<(), FsError> {
    let path = lock_file_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
