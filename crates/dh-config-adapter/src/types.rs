//! Shared types: scope, render result, diff, backup id.

use std::path::{Path, PathBuf};

/// Scope at which an adapter operates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigScope {
    /// User-level configuration (e.g. `~/.claude/settings.json`).
    Global,
    /// Project-level configuration rooted at the given workspace.
    Project(PathBuf),
}

impl ConfigScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigScope::Global => "global",
            ConfigScope::Project(_) => "project",
        }
    }
    pub fn workspace(&self) -> Option<&Path> {
        match self {
            ConfigScope::Global => None,
            ConfigScope::Project(p) => Some(p.as_path()),
        }
    }
}

/// One file an adapter wants to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedFile {
    pub path: PathBuf,
    pub content: Vec<u8>,
    /// Names of the keys/sections inside this file that dh manages. Adapters
    /// preserve any user-authored fields outside of this list.
    pub managed_keys: Vec<String>,
}

impl RenderedFile {
    pub fn new(path: PathBuf, content: Vec<u8>) -> Self {
        Self {
            path,
            content,
            managed_keys: Vec::new(),
        }
    }
    pub fn with_managed_keys(mut self, keys: Vec<String>) -> Self {
        self.managed_keys = keys;
        self
    }
}

/// Output of [`crate::AgentConfigAdapter::render`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderResult {
    pub files: Vec<RenderedFile>,
}

impl RenderResult {
    pub fn push(&mut self, file: RenderedFile) {
        self.files.push(file);
    }
    pub fn iter_paths(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(|f| f.path.as_path())
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Per-file diff classification produced by [`crate::diff::compute`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileChange {
    Created,
    Modified { previous_size: usize, new_size: usize },
    Unchanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub path: PathBuf,
    pub change: FileChange,
}

/// Identifier returned after a successful apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupId(pub String);

impl BackupId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
