//! Computes file diffs by comparing rendered content against on-disk state.

use crate::error::{AdapterError, Result};
use crate::types::{FileChange, FileDiff, RenderResult};
use std::fs;
use std::path::Path;

/// Computes a [`FileDiff`] for every file in `render` against the current
/// filesystem state. Pure read-only — does not write anything.
pub fn compute(render: &RenderResult) -> Result<Vec<FileDiff>> {
    let mut diffs = Vec::with_capacity(render.files.len());
    for file in &render.files {
        diffs.push(diff_one(&file.path, &file.content)?);
    }
    Ok(diffs)
}

fn diff_one(path: &Path, new_content: &[u8]) -> Result<FileDiff> {
    if !path.exists() {
        return Ok(FileDiff {
            path: path.to_path_buf(),
            change: FileChange::Created,
        });
    }
    let existing = fs::read(path).map_err(|source| AdapterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if existing == new_content {
        return Ok(FileDiff {
            path: path.to_path_buf(),
            change: FileChange::Unchanged,
        });
    }
    Ok(FileDiff {
        path: path.to_path_buf(),
        change: FileChange::Modified {
            previous_size: existing.len(),
            new_size: new_content.len(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RenderedFile;
    use tempfile::TempDir;

    fn render(path: &Path, bytes: &[u8]) -> RenderResult {
        let mut r = RenderResult::default();
        r.push(RenderedFile::new(path.to_path_buf(), bytes.to_vec()));
        r
    }

    #[test]
    fn detects_created_modified_unchanged() {
        let tmp = TempDir::new().unwrap();

        // created
        let p1 = tmp.path().join("new.txt");
        let d1 = compute(&render(&p1, b"abc")).unwrap();
        assert_eq!(d1[0].change, FileChange::Created);

        // unchanged
        let p2 = tmp.path().join("same.txt");
        fs::write(&p2, b"abc").unwrap();
        let d2 = compute(&render(&p2, b"abc")).unwrap();
        assert_eq!(d2[0].change, FileChange::Unchanged);

        // modified
        let p3 = tmp.path().join("changed.txt");
        fs::write(&p3, b"old").unwrap();
        let d3 = compute(&render(&p3, b"new-larger")).unwrap();
        match &d3[0].change {
            FileChange::Modified {
                previous_size,
                new_size,
            } => {
                assert_eq!(*previous_size, 3);
                assert_eq!(*new_size, 10);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
