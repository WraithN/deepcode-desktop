//! Agent-config adapter layer.
//!
//! Renders [`dh_config::UnifiedConfig`] into the native configuration files
//! consumed by individual coding agents (Claude Code, OpenCode, …) and ships
//! a reusable apply pipeline (render → diff → backup → atomic write).

pub mod adapter;
pub mod apply;
pub mod claudecode;
pub mod constants;
pub mod diff;
pub mod error;
pub mod io;
pub mod types;

pub use adapter::{AdapterRegistry, AgentConfigAdapter, ApplyOptions, ApplyOutcome};
pub use apply::{apply, apply_with_adapter};
pub use claudecode::ClaudecodeAdapter;
pub use error::{AdapterError, Result};
pub use types::{
    BackupId, ConfigScope, FileChange, FileDiff, RenderResult, RenderedFile,
};

/// Convenience: build a registry pre-populated with every adapter shipped in
/// this crate. Callers that only need a subset can still register manually.
pub fn default_registry() -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(ClaudecodeAdapter::new());
    r
}
