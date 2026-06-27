//! Filesystem-aware loaders for layered config sources.
//!
//! Each loader returns an `Option<UnifiedConfig>`: missing files are treated
//! as "no contribution" rather than errors so that users can opt into any
//! single layer (global only, project only, etc.) without ceremony.

use crate::constants::{
    APP_CONFIG_SUBDIR, CONFIG_FILE_NAME, PROFILES_DIRNAME, PROJECT_CONFIG_DIRNAME,
};
use crate::error::{ConfigError, Result};
use crate::merge::merge_all;
use crate::schema::UnifiedConfig;
use std::path::{Path, PathBuf};

/// Resolves the global dh config directory (`~/.config/dh` on Linux).
///
/// Falls back to `<home>/.dh` if no platform config dir is available.
pub fn global_config_dir() -> Result<PathBuf> {
    if let Some(dir) = dirs::config_dir() {
        return Ok(dir.join(APP_CONFIG_SUBDIR));
    }
    let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
    Ok(home.join(format!(".{APP_CONFIG_SUBDIR}")))
}

/// Returns the project-level dh directory for the given workspace.
pub fn project_config_dir(workspace: impl AsRef<Path>) -> PathBuf {
    workspace.as_ref().join(PROJECT_CONFIG_DIRNAME)
}

/// Loads the global config file if present.
pub fn load_global() -> Result<Option<UnifiedConfig>> {
    let path = global_config_dir()?.join(CONFIG_FILE_NAME);
    load_optional_file(&path)
}

/// Loads a named profile from `~/.config/dh/profiles/<name>.toml`.
pub fn load_profile(name: &str) -> Result<Option<UnifiedConfig>> {
    let path = global_config_dir()?
        .join(PROFILES_DIRNAME)
        .join(format!("{name}.toml"));
    load_optional_file(&path)
}

/// Loads a project-scoped config (`<workspace>/.dh/config.toml`) if present.
pub fn load_project(workspace: impl AsRef<Path>) -> Result<Option<UnifiedConfig>> {
    let path = project_config_dir(workspace).join(CONFIG_FILE_NAME);
    load_optional_file(&path)
}

/// Inputs for [`load_layered`].
#[derive(Debug, Default, Clone)]
pub struct LoadOptions {
    /// Optional explicit profile override; falls back to `default_profile`
    /// from the global file.
    pub profile: Option<String>,
    /// Workspace root to search for project-level overrides.
    pub workspace: Option<PathBuf>,
}

/// Loads global + profile + project layers and merges them into a single
/// configuration document.
///
/// Returns the merged config along with the path of every layer actually
/// contributing to the result, primarily for diagnostics.
pub fn load_layered(opts: &LoadOptions) -> Result<LoadedConfig> {
    let mut layers: Vec<UnifiedConfig> = Vec::new();
    let mut sources: Vec<PathBuf> = Vec::new();

    let global_path = global_config_dir()?.join(CONFIG_FILE_NAME);
    let global = load_optional_file(&global_path)?;
    let profile_name = pick_profile(&global, opts.profile.as_deref());
    if let Some(cfg) = global {
        layers.push(cfg);
        sources.push(global_path);
    }

    if let Some(name) = profile_name.as_deref() {
        let path = global_config_dir()?
            .join(PROFILES_DIRNAME)
            .join(format!("{name}.toml"));
        if let Some(cfg) = load_optional_file(&path)? {
            layers.push(cfg);
            sources.push(path);
        }
    }

    if let Some(workspace) = &opts.workspace {
        let path = project_config_dir(workspace).join(CONFIG_FILE_NAME);
        if let Some(cfg) = load_optional_file(&path)? {
            layers.push(cfg);
            sources.push(path);
        }
    }

    Ok(LoadedConfig {
        config: merge_all(layers),
        sources,
        profile: profile_name,
    })
}

/// Result of a layered load including sources that contributed to the merge.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: UnifiedConfig,
    pub sources: Vec<PathBuf>,
    pub profile: Option<String>,
}

// ───── private helpers ─────

fn load_optional_file(path: &Path) -> Result<Option<UnifiedConfig>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let cfg: UnifiedConfig = toml::from_str(&raw).map_err(|source| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(cfg))
}

/// Picks the profile name to load, preferring the explicit override, then the
/// `default_profile` declared in the global config.
fn pick_profile(global: &Option<UnifiedConfig>, explicit: Option<&str>) -> Option<String> {
    if let Some(name) = explicit {
        return Some(name.to_string());
    }
    global
        .as_ref()
        .and_then(|cfg| cfg.default_profile.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_optional_file_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nope.toml");
        assert!(load_optional_file(&path).unwrap().is_none());
    }

    #[test]
    fn load_optional_file_parses() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("c.toml");
        std::fs::write(&path, "default_profile = \"x\"\n").unwrap();
        let cfg = load_optional_file(&path).unwrap().unwrap();
        assert_eq!(cfg.default_profile.as_deref(), Some("x"));
    }

    #[test]
    fn load_optional_file_reports_parse_error() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bad.toml");
        std::fs::write(&path, "not = valid = toml").unwrap();
        let err = load_optional_file(&path).unwrap_err();
        assert!(matches!(err, ConfigError::ParseToml { .. }));
    }

    #[test]
    fn pick_profile_prefers_explicit() {
        let mut global = UnifiedConfig::default();
        global.default_profile = Some("g".into());
        let chosen = pick_profile(&Some(global), Some("explicit"));
        assert_eq!(chosen.as_deref(), Some("explicit"));
    }
}
