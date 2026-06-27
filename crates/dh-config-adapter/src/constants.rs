//! Constants used across adapter implementations.

/// Top-level key embedded in JSON files indicating they were last written by
/// dh. Adapters use this to detect their own previous output and avoid
/// clobbering user-authored content.
pub const SENTINEL_KEY: &str = "_dhManaged";

/// JSON key listing which fields dh currently owns inside a managed file.
pub const MANAGED_KEYS_KEY: &str = "_dhManagedKeys";

/// Marker line written to markdown files (e.g. CLAUDE.md) wrapping the
/// dh-managed section.
pub const MARKDOWN_BEGIN_MARKER: &str = "<!-- dh:begin -->";
pub const MARKDOWN_END_MARKER: &str = "<!-- dh:end -->";

/// Subdirectory used to store pre-apply backups under the global dh dir.
pub const BACKUPS_DIRNAME: &str = "backups";

/// File suffix appended to atomic temp files before rename.
pub const TEMP_SUFFIX: &str = ".dh-tmp";

// ───── Claude Code paths ─────

/// Per-user Claude Code config directory (relative to the home dir).
pub const CLAUDE_USER_DIRNAME: &str = ".claude";

/// File name of the Claude Code settings JSON in both user and project scopes.
pub const CLAUDE_SETTINGS_FILE: &str = "settings.json";

/// Per-project Claude Code directory placed inside a repo (`<repo>/.claude`).
pub const CLAUDE_PROJECT_DIRNAME: &str = ".claude";

/// Markdown file holding engineering rules / memory in the project root.
pub const CLAUDE_MEMORY_FILE: &str = "CLAUDE.md";

/// Subdirectory under `.claude/` storing slash command (skill) files.
pub const CLAUDE_COMMANDS_DIRNAME: &str = "commands";

// ───── Default render limits ─────

/// Maximum number of files an adapter is allowed to emit in one render to
/// guard against pathological configs (used as a soft cap).
pub const MAX_RENDERED_FILES: usize = 256;
