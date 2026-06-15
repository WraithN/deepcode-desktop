pub(crate) const LOG_SOURCE: &str = "claude-plugin";

pub(crate) const PLUGIN_KEY: &str = "claude-code";
pub(crate) const PLUGIN_NAME: &str = "Claude Code";

// Program and CLI flags.
pub(crate) const PROGRAM_CLAUDE: &str = "claude";
pub(crate) const VERSION_FLAG: &str = "--version";
pub(crate) const PROMPT_FLAG: &str = "-p";
pub(crate) const INPUT_FORMAT_FLAG: &str = "--input-format=stream-json";
pub(crate) const OUTPUT_FORMAT_FLAG: &str = "--output-format=stream-json";
pub(crate) const VERBOSE_FLAG: &str = "--verbose";
pub(crate) const PERMISSION_MODE_PREFIX: &str = "--permission-mode=";
pub(crate) const MODEL_PREFIX: &str = "--model=";
pub(crate) const WORKTREE_PREFIX: &str = "--worktree=";
pub(crate) const RESUME_PREFIX: &str = "--resume=";
pub(crate) const DEFAULT_PERMISSION_MODE: &str = "bypassPermissions";

// Outgoing stream-json message shape.
pub(crate) const PAYLOAD_TYPE_MESSAGE: &str = "message";
pub(crate) const ROLE_USER: &str = "user";
pub(crate) const CONTENT_TYPE_TEXT: &str = "text";
pub(crate) const KEY_TYPE: &str = "type";
pub(crate) const KEY_ROLE: &str = "role";
pub(crate) const KEY_CONTENT: &str = "content";
pub(crate) const KEY_TEXT: &str = "text";
pub(crate) const KEY_SESSION_ID: &str = "session_id";

// Event-sink routing keys.
pub(crate) const METHOD_STATUS_CHANGED: &str = "agent:status_changed";
pub(crate) const KEY_INSTANCE_ID: &str = "instance_id";
pub(crate) const KEY_STATUS: &str = "status";

// Init / runtime tuning.
pub(crate) const SUBTYPE_INIT: &str = "init";
pub(crate) const INIT_TIMEOUT_SECS: u64 = 30;
pub(crate) const RECEIVE_TIMEOUT_MS: u64 = 200;

// Error messages.
pub(crate) const ERR_NO_ACTIVE_SESSION: &str = "no active claude session";
pub(crate) const ERR_SEND_FAILED: &str = "failed to send message to claude";
pub(crate) const ERR_INIT_TIMEOUT: &str = "timed out waiting for claude init event";
pub(crate) const ERR_START_FAILED: &str = "failed to start claude process";

// Lifecycle log messages.
pub(crate) const LOG_STARTED: &str = "claude process started";
pub(crate) const LOG_STOPPED: &str = "claude process stopped";

// PID placeholder because we do not track the real OS pid.
pub(crate) const UNKNOWN_PID: u32 = 0;
