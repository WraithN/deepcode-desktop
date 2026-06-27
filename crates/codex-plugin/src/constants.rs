pub(crate) const LOG_SOURCE: &str = "codex-plugin";

pub(crate) const PLUGIN_KEY: &str = "codex";
pub(crate) const PLUGIN_NAME: &str = "Codex";

// Program and CLI flags.
pub(crate) const PROGRAM_CODEX: &str = "codex";
pub(crate) const VERSION_FLAG: &str = "--version";
pub(crate) const APP_SERVER_CMD: &str = "app-server";
pub(crate) const LISTEN_FLAG: &str = "--listen";
pub(crate) const LISTEN_STDIO: &str = "stdio://";

// JSON-RPC framing. Codex app-server omits the standard "jsonrpc":"2.0" field
// on the wire, so we keep payloads minimal and compatible.
pub(crate) const KEY_ID: &str = "id";
pub(crate) const KEY_METHOD: &str = "method";
pub(crate) const KEY_PARAMS: &str = "params";
pub(crate) const KEY_RESULT: &str = "result";
pub(crate) const KEY_MESSAGE: &str = "message";

// App-server lifecycle methods.
pub(crate) const METHOD_INITIALIZE: &str = "initialize";
pub(crate) const METHOD_INITIALIZED: &str = "initialized";
pub(crate) const METHOD_THREAD_START: &str = "thread/start";
pub(crate) const METHOD_TURN_START: &str = "turn/start";

// App-server notification / response keys.
pub(crate) const KEY_THREAD_ID: &str = "thread_id";
pub(crate) const KEY_THREAD: &str = "thread";
pub(crate) const KEY_INPUT: &str = "input";
pub(crate) const KEY_TYPE: &str = "type";
pub(crate) const KEY_TEXT: &str = "text";
pub(crate) const KEY_DELTA: &str = "delta";
pub(crate) const KEY_CONTENT: &str = "content";

// Turn / item event types.
pub(crate) const EVENT_TURN_STARTED: &str = "turn/started";
pub(crate) const EVENT_TURN_COMPLETED: &str = "turn/completed";
pub(crate) const EVENT_ITEM_AGENT_MESSAGE_DELTA: &str = "item/agentMessage/delta";
pub(crate) const EVENT_ITEM_COMMAND_EXECUTION_OUTPUT_DELTA: &str = "item/commandExecution/outputDelta";
pub(crate) const EVENT_ITEM_FILE_CHANGE: &str = "item/fileChange";
pub(crate) const EVENT_ERROR: &str = "error";

// Runtime tuning.
pub(crate) const RECEIVE_TIMEOUT_MS: u64 = 200;
pub(crate) const REQUEST_TIMEOUT_MS: u64 = 5_000;

// Error / log messages.
pub(crate) const ERR_START_FAILED: &str = "failed to start codex app-server";
pub(crate) const ERR_SEND_FAILED: &str = "failed to send message to codex";
pub(crate) const ERR_NOT_INITIALIZED: &str = "codex app-server not initialized";
pub(crate) const ERR_THREAD_START_FAILED: &str = "failed to start codex thread";
pub(crate) const ERR_REQUEST_TIMEOUT: &str = "codex app-server request timed out";

pub(crate) const LOG_STARTED: &str = "codex app-server started";
pub(crate) const LOG_STOPPED: &str = "codex app-server stopped";
