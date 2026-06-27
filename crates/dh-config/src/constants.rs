//! Constants for the dh-config crate.
//!
//! Centralizes filenames, directory names, default values, and TOML keys to
//! keep the rest of the crate free of magic strings.

// ───── Directory & file names ─────

/// Subdirectory under the user config dir (e.g. `~/.config`) holding global dh config.
pub const APP_CONFIG_SUBDIR: &str = "dh";

/// File name of the main config file in the global / project scope.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Subdirectory storing user-level skill files.
pub const SKILLS_DIRNAME: &str = "skills";

/// Subdirectory storing rule files (markdown).
pub const RULES_DIRNAME: &str = "rules";

/// Subdirectory storing per-profile overrides.
pub const PROFILES_DIRNAME: &str = "profiles";

/// Subdirectory storing pre-apply backups of native agent configs.
pub const BACKUPS_DIRNAME: &str = "backups";

/// Project-level dh config directory placed inside a repository (`<repo>/.dh`).
pub const PROJECT_CONFIG_DIRNAME: &str = ".dh";

// ───── Defaults ─────

/// Default profile name when no profile is selected.
pub const DEFAULT_PROFILE: &str = "default";

// ───── Placeholder syntax ─────

/// Opening sequence for placeholders, e.g. `${env:NAME}`.
pub const PLACEHOLDER_OPEN: &str = "${";

/// Closing character for placeholders.
pub const PLACEHOLDER_CLOSE: char = '}';

/// Separator between placeholder kind and value (`env:NAME`).
pub const PLACEHOLDER_KIND_SEP: char = ':';

/// Placeholder kind reading an environment variable.
pub const PLACEHOLDER_KIND_ENV: &str = "env";

/// Placeholder kind reading a value from the OS keyring.
pub const PLACEHOLDER_KIND_KEYRING: &str = "keyring";

/// Bare placeholder name expanding to the resolved workspace path.
pub const PLACEHOLDER_WORKSPACE: &str = "workspace";

// ───── Validation limits ─────

/// Maximum recursion depth allowed when expanding nested placeholders.
pub const MAX_PLACEHOLDER_DEPTH: usize = 8;
