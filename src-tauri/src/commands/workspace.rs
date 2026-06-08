use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use base64::Engine;

#[tauri::command]
pub fn get_current_dir() -> Result<String, String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct WorkspaceFileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub ignored: bool,
    pub children: Option<Vec<WorkspaceFileNode>>,
}

#[derive(serde::Serialize)]
pub struct WorkspaceFileContent {
    pub path: String,
    pub content: String,
    pub truncated: bool,
    pub is_image: bool,
}

fn ignored_workspace_entry(name: &str) -> bool {
    matches!(name, "node_modules" | ".git" | "target" | "dist" | ".dist" | ".next" | "build")
}

pub fn resolve_workspace_path(workspace: &str, relative_path: Option<&str>) -> Result<PathBuf, String> {
    let root = PathBuf::from(workspace).canonicalize().map_err(|e| format!("工作区不存在: {}", e))?;
    let target = match relative_path {
        Some(path) if !path.is_empty() => root.join(path),
        _ => root.clone(),
    };
    let canonical = target.canonicalize().map_err(|e| format!("路径不存在: {}", e))?;
    if !canonical.starts_with(&root) {
        return Err("禁止访问工作区外的路径".to_string());
    }
    Ok(canonical)
}

fn path_to_workspace_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn git_ignored(root: &Path, relative_path: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("check-ignore")
        .arg("-q")
        .arg(relative_path)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn is_image_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".gif") || lower.ends_with(".webp") || lower.ends_with(".svg") || lower.ends_with(".bmp")
}

fn image_mime(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/png"
    }
}

fn read_workspace_dir(root: &Path, dir: &Path, depth: usize, max_depth: usize) -> Result<Vec<WorkspaceFileNode>, String> {
    if depth > max_depth {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut nodes = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if ignored_workspace_entry(&name) {
            continue;
        }

        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let is_dir = metadata.is_dir();
        let relative_path = path_to_workspace_relative(root, &path);
        let ignored = git_ignored(root, &relative_path);
        let children = if is_dir && depth < max_depth {
            Some(read_workspace_dir(root, &path, depth + 1, max_depth)?)
        } else {
            None
        };

        nodes.push(WorkspaceFileNode {
            name,
            path: relative_path,
            is_dir,
            ignored,
            children,
        });
    }

    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(nodes)
}

#[tauri::command]
pub fn list_workspace_tree(workspace: String) -> Result<Vec<WorkspaceFileNode>, String> {
    let root = resolve_workspace_path(&workspace, None)?;
    read_workspace_dir(&root, &root, 0, 4)
}

#[tauri::command]
pub fn read_workspace_file(workspace: String, path: String) -> Result<WorkspaceFileContent, String> {
    let root = PathBuf::from(&workspace).canonicalize().map_err(|e| format!("工作区不存在: {}", e))?;
    let target = resolve_workspace_path(&workspace, Some(&path))?;
    let metadata = fs::metadata(&target).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("只能读取文件".to_string());
    }

    let relative_path = path_to_workspace_relative(&root, &target);
    let bytes = fs::read(&target).map_err(|e| e.to_string())?;

    if is_image_path(&relative_path) {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(WorkspaceFileContent {
            path: relative_path.clone(),
            content: format!("data:{};base64,{}", image_mime(&relative_path), encoded),
            truncated: false,
            is_image: true,
        });
    }

    let max_size = 512 * 1024;
    let truncated = bytes.len() > max_size;
    let readable = if truncated { &bytes[..max_size] } else { &bytes[..] };
    let content = String::from_utf8_lossy(readable).to_string();

    Ok(WorkspaceFileContent {
        path: relative_path,
        content,
        truncated,
        is_image: false,
    })
}
