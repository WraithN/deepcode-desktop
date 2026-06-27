//! Unified configuration layer for dh.
//!
//! This crate is the single source of truth for the layered configuration the
//! `dh` CLI maintains on behalf of users. Higher-level adapters render this
//! schema into agent-native configuration files (e.g. `opencode.json`,
//! `~/.claude.json`, `<repo>/CLAUDE.md`).
//!
//! ## Layers
//!
//! Configuration is composed from up to three layers, listed from lowest to
//! highest precedence:
//!
//! 1. Global config — `~/.config/dh/config.toml`
//! 2. Profile override — `~/.config/dh/profiles/<name>.toml`
//! 3. Project config — `<workspace>/.dh/config.toml`
//!
//! ## Pipeline
//!
//! The typical pipeline is:
//!
//! ```ignore
//! let loaded = dh_config::load_layered(&LoadOptions {
//!     workspace: Some(std::env::current_dir().unwrap()),
//!     ..Default::default()
//! })?;
//! validate(&loaded.config).into_result()?;
//! // Adapters now read `loaded.config` and write agent-native files.
//! ```

pub mod constants;
pub mod error;
pub mod expand;
pub mod loader;
pub mod merge;
pub mod placeholder;
pub mod schema;
pub mod validate;

pub use error::{ConfigError, Result};
pub use expand::expand_config;
pub use loader::{
    global_config_dir, load_global, load_layered, load_profile, load_project, project_config_dir,
    LoadOptions, LoadedConfig,
};
pub use merge::{merge, merge_all};
pub use placeholder::{
    expand, ExpandContext, InMemoryKeyringResolver, KeyringResolver, NoopKeyringResolver,
};
pub use schema::{
    McpServerConfig, ModelConfig, ProviderConfig, RulesConfig, SkillsConfig, UnifiedConfig,
};
pub use validate::{validate, Issue, ValidationReport};
