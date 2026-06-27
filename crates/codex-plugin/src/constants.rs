pub(crate) const LOG_SOURCE: &str = "codex-plugin";

pub(crate) const PLUGIN_KEY: &str = "codex";
pub(crate) const PLUGIN_NAME: &str = "Codex";

// Program and CLI flags.
pub(crate) const PROGRAM_CODEX: &str = "codex";
pub(crate) const VERSION_FLAG: &str = "--version";
pub(crate) const APP_SERVER_SUBCOMMAND: &str = "app-server";
pub(crate) const STDIO_FLAG: &str = "--stdio";

// JSON-RPC lifecycle methods.
pub(crate) const METHOD_INITIALIZE: &str = "initialize";
pub(crate) const METHOD_INITIALIZED: &str = "initialized";
pub(crate) const METHOD_THREAD_START: &str = "thread/start";
pub(crate) const METHOD_TURN_START: &str = "turn/start";

// Default runtime tuning.
pub(crate) const REQUEST_TIMEOUT_SECS: u64 = 10;
pub(crate) const RECEIVE_TIMEOUT_MS: u64 = 200;

// Error messages.
pub(crate) const ERR_NO_ACTIVE_THREAD: &str = "no active codex thread";
pub(crate) const ERR_SEND_FAILED: &str = "failed to send message to codex";
pub(crate) const ERR_INIT_TIMEOUT: &str = "timed out waiting for codex initialization";
pub(crate) const ERR_START_FAILED: &str = "failed to start codex app-server";

// Lifecycle log messages.
pub(crate) const LOG_STARTED: &str = "codex app-server started";
pub(crate) const LOG_STOPPED: &str = "codex app-server stopped";
