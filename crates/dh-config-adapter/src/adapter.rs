//! Core trait and [`AdapterRegistry`] for agent-config adapters.

use crate::error::Result;
use crate::types::{BackupId, ConfigScope, RenderResult};
use dh_config::UnifiedConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Renders the unified config into agent-native files and applies them.
///
/// Implementations must be deterministic: given the same inputs they must
/// produce identical [`RenderResult`]s so dry-runs and diffs stay meaningful.
pub trait AgentConfigAdapter: Send + Sync {
    /// Stable key (e.g. `"claudecode"`, `"opencode"`).
    fn key(&self) -> &'static str;

    /// Human-readable name for CLI output.
    fn display_name(&self) -> &'static str;

    /// Lists every file path the adapter would touch in `scope`.
    /// Useful for `dh config apply --files`.
    fn target_paths(&self, scope: &ConfigScope) -> Vec<PathBuf>;

    /// Renders the unified configuration into the adapter's native format.
    fn render(&self, cfg: &UnifiedConfig, scope: &ConfigScope) -> Result<RenderResult>;
}

/// Registry of all known adapters keyed by [`AgentConfigAdapter::key`].
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<&'static str, Box<dyn AgentConfigAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<A: AgentConfigAdapter + 'static>(&mut self, adapter: A) {
        self.adapters.insert(adapter.key(), Box::new(adapter));
    }

    pub fn get(&self, key: &str) -> Option<&dyn AgentConfigAdapter> {
        self.adapters.get(key).map(|b| b.as_ref())
    }

    pub fn keys(&self) -> Vec<&'static str> {
        self.adapters.keys().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

/// Options forwarded to [`crate::apply::apply`].
#[derive(Clone, Debug, Default)]
pub struct ApplyOptions {
    /// When true, no files are written; the call still produces a diff.
    pub dry_run: bool,
    /// Override the directory where backups are placed (defaults to the
    /// global dh config dir).
    pub backup_dir: Option<PathBuf>,
}

/// High-level result from applying a single adapter.
#[derive(Clone, Debug)]
pub struct ApplyOutcome {
    pub adapter: &'static str,
    pub backup_id: Option<BackupId>,
    pub diffs: Vec<crate::types::FileDiff>,
}
