//! Workspace path validation to prevent path traversal and symlink escape.

use std::path::PathBuf;

const ENV_WORKSPACE_ROOTS: &str = "GATEWAYD_WORKSPACE_ROOTS";

/// Error returned when a workspace path fails validation.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceValidationError {
    #[error("work_directory is required")]
    Missing,
    #[error("invalid work_directory '{0}': {1}")]
    InvalidPath(String, String),
    #[error("work_directory '{0}' is not a directory")]
    NotADirectory(String),
    #[error("work_directory '{0}' is outside allowed workspace roots")]
    OutsideAllowedRoots(String),
}

/// Validates and canonicalizes workspace paths.
///
/// By default the current working directory of the gatewayd process is the
/// only allowed root. Additional roots can be configured via the
/// `GATEWAYD_WORKSPACE_ROOTS` environment variable as a comma-separated list.
#[derive(Clone, Debug)]
pub struct WorkspaceValidator {
    roots: Vec<PathBuf>,
}

impl WorkspaceValidator {
    /// Create a validator from the current environment.
    pub fn from_env() -> Self {
        let mut roots = Vec::new();
        if let Ok(val) = std::env::var(ENV_WORKSPACE_ROOTS) {
            for raw in val.split(',') {
                let raw = raw.trim();
                if raw.is_empty() {
                    continue;
                }
                if let Ok(canonical) = std::fs::canonicalize(raw) {
                    roots.push(canonical);
                } else {
                    // Keep the raw path as a fallback so the validator can
                    // still report a meaningful error later.
                    roots.push(PathBuf::from(raw));
                }
            }
        }
        if roots.is_empty() {
            if let Ok(cwd) = std::env::current_dir() {
                if let Ok(canonical) = std::fs::canonicalize(&cwd) {
                    roots.push(canonical);
                } else {
                    roots.push(cwd);
                }
            }
        }
        Self { roots }
    }

    /// Validate that `work_directory` is a real directory inside one of the
    /// allowed roots. Returns the canonicalized path on success.
    pub fn validate(&self, work_directory: &str) -> Result<PathBuf, WorkspaceValidationError> {
        if work_directory.is_empty() {
            return Err(WorkspaceValidationError::Missing);
        }

        let canonical = std::fs::canonicalize(work_directory).map_err(|e| {
            WorkspaceValidationError::InvalidPath(work_directory.to_string(), e.to_string())
        })?;

        if !canonical.is_dir() {
            return Err(WorkspaceValidationError::NotADirectory(
                canonical.display().to_string(),
            ));
        }

        if !self.roots.iter().any(|root| canonical.starts_with(root)) {
            return Err(WorkspaceValidationError::OutsideAllowedRoots(
                canonical.display().to_string(),
            ));
        }

        Ok(canonical)
    }
}

impl Default for WorkspaceValidator {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_rejects_empty() {
        let validator = WorkspaceValidator {
            roots: vec![PathBuf::from("/tmp")],
        };
        assert!(matches!(
            validator.validate(""),
            Err(WorkspaceValidationError::Missing)
        ));
    }

    #[test]
    fn test_validator_rejects_nonexistent() {
        let validator = WorkspaceValidator {
            roots: vec![PathBuf::from("/tmp")],
        };
        assert!(matches!(
            validator.validate("/definitely/does/not/exist"),
            Err(WorkspaceValidationError::InvalidPath(_, _))
        ));
    }

    #[test]
    fn test_validator_accepts_allowed_root() {
        let validator = WorkspaceValidator {
            roots: vec![PathBuf::from("/tmp")],
        };
        let result = validator.validate("/tmp");
        assert!(result.is_ok(), "{:?}", result);
    }
}
