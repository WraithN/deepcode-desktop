//! Apply pipeline: render → diff → backup → write → return outcome.

use crate::adapter::{AdapterRegistry, AgentConfigAdapter, ApplyOptions, ApplyOutcome};
use crate::diff;
use crate::error::Result;
use crate::io::{backup_existing, new_backup_id, write_all};
use crate::types::ConfigScope;
use dh_config::{global_config_dir, UnifiedConfig};
use std::path::PathBuf;

/// Renders the unified config through the named adapter and writes the result
/// to disk (unless `opts.dry_run` is set), backing up any pre-existing files.
pub fn apply(
    registry: &AdapterRegistry,
    adapter_key: &str,
    cfg: &UnifiedConfig,
    scope: &ConfigScope,
    opts: &ApplyOptions,
) -> Result<ApplyOutcome> {
    let adapter = registry.get(adapter_key).ok_or_else(|| {
        crate::error::AdapterError::Unsupported(format!("unknown adapter: {adapter_key}"))
    })?;
    apply_with_adapter(adapter, cfg, scope, opts)
}

/// Variant of [`apply`] that takes a concrete adapter reference. Useful in
/// tests that want to bypass the registry.
pub fn apply_with_adapter(
    adapter: &dyn AgentConfigAdapter,
    cfg: &UnifiedConfig,
    scope: &ConfigScope,
    opts: &ApplyOptions,
) -> Result<ApplyOutcome> {
    let render = adapter.render(cfg, scope)?;
    let diffs = diff::compute(&render)?;

    if opts.dry_run {
        return Ok(ApplyOutcome {
            adapter: adapter.key(),
            backup_id: None,
            diffs,
        });
    }

    let backup_root = resolve_backup_root(opts)?;
    let id = new_backup_id(adapter.key());
    let _backup_dir = backup_existing(&backup_root, adapter.key(), &id, &render)?;
    write_all(&render)?;

    Ok(ApplyOutcome {
        adapter: adapter.key(),
        backup_id: Some(id),
        diffs,
    })
}

fn resolve_backup_root(opts: &ApplyOptions) -> Result<PathBuf> {
    if let Some(path) = &opts.backup_dir {
        return Ok(path.clone());
    }
    Ok(global_config_dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileChange, RenderResult, RenderedFile};
    use std::path::Path;
    use tempfile::TempDir;

    struct DummyAdapter {
        target: PathBuf,
        content: Vec<u8>,
    }
    impl AgentConfigAdapter for DummyAdapter {
        fn key(&self) -> &'static str {
            "dummy"
        }
        fn display_name(&self) -> &'static str {
            "Dummy"
        }
        fn target_paths(&self, _scope: &ConfigScope) -> Vec<PathBuf> {
            vec![self.target.clone()]
        }
        fn render(&self, _cfg: &UnifiedConfig, _scope: &ConfigScope) -> Result<RenderResult> {
            let mut r = RenderResult::default();
            r.push(RenderedFile::new(self.target.clone(), self.content.clone()));
            Ok(r)
        }
    }

    #[test]
    fn dry_run_does_not_write() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("out.json");
        let adapter = DummyAdapter {
            target: target.clone(),
            content: b"hello".to_vec(),
        };
        let opts = ApplyOptions {
            dry_run: true,
            backup_dir: Some(tmp.path().join("backups")),
        };
        let outcome = apply_with_adapter(
            &adapter,
            &UnifiedConfig::default(),
            &ConfigScope::Global,
            &opts,
        )
        .unwrap();
        assert!(outcome.backup_id.is_none());
        assert_eq!(outcome.diffs[0].change, FileChange::Created);
        assert!(!Path::new(&target).exists());
    }

    #[test]
    fn writes_and_records_backup() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("out.json");
        let adapter = DummyAdapter {
            target: target.clone(),
            content: b"hello".to_vec(),
        };
        let opts = ApplyOptions {
            dry_run: false,
            backup_dir: Some(tmp.path().join("backups")),
        };
        let outcome = apply_with_adapter(
            &adapter,
            &UnifiedConfig::default(),
            &ConfigScope::Global,
            &opts,
        )
        .unwrap();
        assert!(outcome.backup_id.is_some());
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }
}
