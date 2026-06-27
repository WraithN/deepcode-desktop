//! Atomic file writer + backup manager shared by all adapters.

use crate::constants::{BACKUPS_DIRNAME, TEMP_SUFFIX};
use crate::error::{AdapterError, Result};
use crate::types::{BackupId, RenderResult};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Generates a fresh backup id based on the current UTC timestamp.
pub fn new_backup_id(adapter_key: &str) -> BackupId {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ");
    BackupId(format!("{adapter_key}-{ts}"))
}

/// Resolves the backup directory for `adapter_key` under `root`.
pub fn backup_dir_for(root: &Path, adapter_key: &str, id: &BackupId) -> PathBuf {
    root.join(BACKUPS_DIRNAME).join(adapter_key).join(&id.0)
}

/// Backs up every existing file in `render` into the backup directory and
/// returns the backup root.
pub fn backup_existing(
    root: &Path,
    adapter_key: &str,
    id: &BackupId,
    render: &RenderResult,
) -> Result<Option<PathBuf>> {
    if !render.files.iter().any(|f| f.path.exists()) {
        return Ok(None);
    }
    let dir = backup_dir_for(root, adapter_key, id);
    create_dir_all(&dir)?;
    for file in &render.files {
        if !file.path.exists() {
            continue;
        }
        let dest = mirror_path(&dir, &file.path);
        if let Some(parent) = dest.parent() {
            create_dir_all(parent)?;
        }
        fs::copy(&file.path, &dest).map_err(|e| AdapterError::Io {
            path: file.path.clone(),
            source: e,
        })?;
    }
    Ok(Some(dir))
}

/// Writes every rendered file to disk atomically (`*.dh-tmp` + rename).
pub fn write_all(render: &RenderResult) -> Result<()> {
    for file in &render.files {
        if let Some(parent) = file.path.parent() {
            create_dir_all(parent)?;
        }
        write_atomic(&file.path, &file.content)?;
    }
    Ok(())
}

/// Restores all files captured in a previous backup into their original
/// locations. Files added by the apply that were not present before will be
/// missing from the backup; those are left in place by this function — the
/// caller is responsible for explicit deletion if desired.
pub fn restore_from(backup_root: &Path) -> Result<Vec<PathBuf>> {
    let mut restored = Vec::new();
    walk_files(backup_root, &mut |entry| {
        let original = unmirror_path(backup_root, entry);
        if let Some(parent) = original.parent() {
            create_dir_all(parent)?;
        }
        fs::copy(entry, &original).map_err(|e| AdapterError::Io {
            path: original.clone(),
            source: e,
        })?;
        restored.push(original);
        Ok(())
    })?;
    Ok(restored)
}

// ───── private helpers ─────

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = with_suffix(path, TEMP_SUFFIX);
    {
        let mut file = fs::File::create(&tmp).map_err(|e| AdapterError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        file.write_all(bytes).map_err(|e| AdapterError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        file.sync_all().map_err(|e| AdapterError::Io {
            path: tmp.clone(),
            source: e,
        })?;
    }
    fs::rename(&tmp, path).map_err(|e| AdapterError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| AdapterError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Maps an absolute target path into the backup tree, preserving the original
/// path components after collapsing the leading `/` (or drive letter).
fn mirror_path(root: &Path, target: &Path) -> PathBuf {
    let stripped = strip_root(target);
    root.join(stripped)
}

fn unmirror_path(root: &Path, entry: &Path) -> PathBuf {
    let rel = entry.strip_prefix(root).unwrap_or(entry);
    let mut out = PathBuf::from("/");
    out.push(rel);
    out
}

fn strip_root(target: &Path) -> PathBuf {
    let mut comps = target.components();
    if let Some(std::path::Component::RootDir) = comps.clone().next() {
        comps.next();
    }
    comps.as_path().to_path_buf()
}

fn walk_files<F>(root: &Path, visit: &mut F) -> Result<()>
where
    F: FnMut(&Path) -> Result<()>,
{
    if !root.exists() {
        return Ok(());
    }
    let read = fs::read_dir(root).map_err(|e| AdapterError::Io {
        path: root.to_path_buf(),
        source: e,
    })?;
    for entry in read {
        let entry = entry.map_err(|e| AdapterError::Io {
            path: root.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, visit)?;
        } else {
            visit(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RenderedFile;
    use tempfile::TempDir;

    #[test]
    fn write_then_restore_round_trip() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("a/b/c.txt");
        let backup_root = tmp.path().join("backups-root");

        // First write: original content.
        let mut render = RenderResult::default();
        render.push(RenderedFile::new(target.clone(), b"v1".to_vec()));
        write_all(&render).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"v1");

        // Backup before overwriting.
        let id = BackupId("test".into());
        let dir = backup_existing(&backup_root, "claudecode", &id, &render)
            .unwrap()
            .unwrap();
        assert!(dir.exists());

        // Second write: new content.
        let mut render2 = RenderResult::default();
        render2.push(RenderedFile::new(target.clone(), b"v2".to_vec()));
        write_all(&render2).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"v2");

        // Restore from backup.
        let restored = restore_from(&dir).unwrap();
        assert_eq!(restored.len(), 1);
    }

    #[test]
    fn backup_existing_returns_none_when_nothing_to_back_up() {
        let tmp = TempDir::new().unwrap();
        let mut render = RenderResult::default();
        render.push(RenderedFile::new(tmp.path().join("ghost"), b"x".to_vec()));
        let id = BackupId("z".into());
        let result = backup_existing(tmp.path(), "k", &id, &render).unwrap();
        assert!(result.is_none());
    }
}
