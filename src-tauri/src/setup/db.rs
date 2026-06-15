use std::fs;
use std::path::PathBuf;
use tauri::Manager;

pub fn db_path(app_handle: &tauri::App) -> PathBuf {
    let mut path = app_handle.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    fs::create_dir_all(&path).ok();
    path.push("app.db");
    path
}
