//! Path helpers for the Claude Code adapter.
//!
//! Claude Code uses two scopes:
//! * Global: `~/.claude/settings.json`
//! * Project: `<repo>/.claude/settings.json` plus `<repo>/CLAUDE.md` and
//!   `<repo>/.claude/commands/<skill>.md` for slash commands.

use crate::constants::{
    CLAUDE_COMMANDS_DIRNAME, CLAUDE_MEMORY_FILE, CLAUDE_PROJECT_DIRNAME, CLAUDE_SETTINGS_FILE,
    CLAUDE_USER_DIRNAME,
};
use crate::error::{AdapterError, Result};
use crate::types::ConfigScope;
use std::path::PathBuf;

/// Resolves the `settings.json` path for the requested scope.
pub fn settings_path(scope: &ConfigScope) -> Result<PathBuf> {
    match scope {
        ConfigScope::Global => global_settings_path(),
        ConfigScope::Project(workspace) => Ok(workspace
            .join(CLAUDE_PROJECT_DIRNAME)
            .join(CLAUDE_SETTINGS_FILE)),
    }
}

/// Resolves `<repo>/CLAUDE.md` for project scope. Returns an error in global
/// scope because Claude Code does not have a user-wide memory file.
pub fn memory_path(scope: &ConfigScope) -> Result<PathBuf> {
    match scope {
        ConfigScope::Project(workspace) => Ok(workspace.join(CLAUDE_MEMORY_FILE)),
        ConfigScope::Global => Err(AdapterError::UnsupportedScope {
            adapter: "claudecode".into(),
            scope: "global has no memory file".into(),
        }),
    }
}

/// Resolves the slash-command directory for skill files.
pub fn commands_dir(scope: &ConfigScope) -> Result<PathBuf> {
    match scope {
        ConfigScope::Project(workspace) => Ok(workspace
            .join(CLAUDE_PROJECT_DIRNAME)
            .join(CLAUDE_COMMANDS_DIRNAME)),
        ConfigScope::Global => global_commands_dir(),
    }
}

fn global_settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        AdapterError::Unsupported("could not determine home directory".into())
    })?;
    Ok(home
        .join(CLAUDE_USER_DIRNAME)
        .join(CLAUDE_SETTINGS_FILE))
}

fn global_commands_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        AdapterError::Unsupported("could not determine home directory".into())
    })?;
    Ok(home
        .join(CLAUDE_USER_DIRNAME)
        .join(CLAUDE_COMMANDS_DIRNAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_paths() {
        let ws = PathBuf::from("/tmp/repo");
        let scope = ConfigScope::Project(ws);
        assert!(settings_path(&scope)
            .unwrap()
            .ends_with(".claude/settings.json"));
        assert!(memory_path(&scope).unwrap().ends_with("CLAUDE.md"));
        assert!(commands_dir(&scope).unwrap().ends_with(".claude/commands"));
    }

    #[test]
    fn global_memory_unsupported() {
        let err = memory_path(&ConfigScope::Global).unwrap_err();
        assert!(matches!(err, AdapterError::UnsupportedScope { .. }));
    }
}
