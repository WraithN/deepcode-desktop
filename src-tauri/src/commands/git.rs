use std::path::Path;
use std::process::Command;
use crate::commands::workspace::resolve_workspace_path;

#[derive(serde::Serialize, Clone)]
pub struct GitStatusEntry {
    pub path: String,
    pub status: String,
}

#[derive(serde::Serialize)]
pub struct GitChangedFile {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub diff: String,
}

pub fn git_status_entries(root: &Path) -> Result<Vec<GitStatusEntry>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .output()
        .map_err(|e| format!("读取 git 状态失败: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    let parts: Vec<&str> = raw.split('\0').filter(|part| !part.is_empty()).collect();
    let mut index = 0;

    while index < parts.len() {
        let item = parts[index];
        if item.len() < 4 {
            index += 1;
            continue;
        }

        let code = &item[..2];
        let path = item[3..].to_string();
        let status = if code == "??" {
            "U"
        } else if code.contains('A') {
            "A"
        } else if code.contains('D') {
            "D"
        } else if code.contains('R') {
            "R"
        } else if code.contains('M') {
            "M"
        } else {
            "M"
        };

        entries.push(GitStatusEntry { path, status: status.to_string() });
        index += if code.contains('R') || code.contains('C') { 2 } else { 1 };
    }

    Ok(entries)
}

fn git_numstat(root: &Path, path: &str) -> (u64, u64) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--numstat")
        .arg("--")
        .arg(path)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = raw.lines().next() {
                let mut parts = line.split('\t');
                let additions = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let deletions = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                return (additions, deletions);
            }
        }
    }

    (0, 0)
}

fn git_diff(root: &Path, path: &str, status: &str) -> String {
    if status == "U" {
        return String::new();
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--")
        .arg(path)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).to_string();
        }
    }

    String::new()
}

#[tauri::command]
pub fn git_status_workspace(workspace: String) -> Result<Vec<GitStatusEntry>, String> {
    let root = resolve_workspace_path(&workspace, None)?;
    git_status_entries(&root)
}

#[tauri::command]
pub fn git_changed_files(workspace: String) -> Result<Vec<GitChangedFile>, String> {
    let root = resolve_workspace_path(&workspace, None)?;
    let status_entries = git_status_entries(&root)?;

    Ok(status_entries
        .into_iter()
        .map(|entry| {
            let (additions, deletions) = git_numstat(&root, &entry.path);
            let diff = git_diff(&root, &entry.path, &entry.status);
            GitChangedFile {
                path: entry.path,
                status: entry.status,
                additions,
                deletions,
                diff,
            }
        })
        .collect())
}
