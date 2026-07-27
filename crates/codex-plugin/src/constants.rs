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

// Codex app-server notification method names.
pub(crate) const METHOD_ITEM_AGENT_MESSAGE_DELTA: &str = "item/agentMessage/delta";
pub(crate) const METHOD_ITEM_REASONING_SUMMARY_TEXT_DELTA: &str = "item/reasoning/summaryTextDelta";
pub(crate) const METHOD_ITEM_REASONING_TEXT_DELTA: &str = "item/reasoning/textDelta";
pub(crate) const METHOD_ITEM_REASONING_SUMMARY_PART_ADDED: &str = "item/reasoning/summaryPartAdded";
pub(crate) const METHOD_ITEM_STARTED: &str = "item/started";
pub(crate) const METHOD_ITEM_COMPLETED: &str = "item/completed";
pub(crate) const METHOD_TURN_COMPLETED: &str = "turn/completed";
pub(crate) const METHOD_TURN_FAILED: &str = "turn/failed";
pub(crate) const METHOD_ERROR: &str = "error";

// Codex item types.
pub(crate) const ITEM_TYPE_AGENT_MESSAGE: &str = "agent_message";
pub(crate) const ITEM_TYPE_REASONING: &str = "reasoning";
pub(crate) const ITEM_TYPE_COMMAND_EXECUTION: &str = "command_execution";
pub(crate) const ITEM_TYPE_EXEC_COMMAND: &str = "exec_command";
pub(crate) const ITEM_TYPE_SHELL: &str = "shell";
pub(crate) const ITEM_TYPE_MCP_TOOL_CALL: &str = "mcp_tool_call";
pub(crate) const ITEM_TYPE_TOOL_CALL: &str = "tool_call";

// Common JSON keys.
pub(crate) const KEY_THREAD_ID: &str = "threadId";
pub(crate) const KEY_THREAD_ID_ALT: &str = "thread_id";
pub(crate) const KEY_ITEM: &str = "item";
pub(crate) const KEY_ITEM_TYPE: &str = "item_type";
pub(crate) const KEY_TYPE: &str = "type";
pub(crate) const KEY_DELTA: &str = "delta";
pub(crate) const KEY_TEXT: &str = "text";
pub(crate) const KEY_COMMAND: &str = "command";
pub(crate) const KEY_NAME: &str = "name";
pub(crate) const KEY_ARGUMENTS: &str = "arguments";
pub(crate) const KEY_OUTPUT: &str = "output";
pub(crate) const KEY_EXIT_CODE: &str = "exitCode";
pub(crate) const KEY_EXIT_CODE_ALT: &str = "exit_code";
pub(crate) const KEY_FAILED: &str = "failed";
pub(crate) const KEY_SUMMARY: &str = "summary";
pub(crate) const KEY_CONTENT: &str = "content";
pub(crate) const KEY_ERROR: &str = "error";
pub(crate) const KEY_MESSAGE: &str = "message";

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
