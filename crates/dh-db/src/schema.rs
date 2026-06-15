pub const CREATE_AUDIT_LOGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    direction TEXT NOT NULL CHECK(direction IN ('request', 'response')),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    agent_type TEXT,
    payload TEXT,
    payload_size_bytes INTEGER NOT NULL DEFAULT 0,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    total_tokens INTEGER,
    timestamp TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_logs(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_request ON audit_logs(request_id);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_logs(timestamp);
"#;

pub const CREATE_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    agent_type TEXT NOT NULL,
    model TEXT NOT NULL,
    workspace TEXT,
    started_at TEXT NOT NULL,
    last_active_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active', 'idle', 'closed'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
"#;

pub const CREATE_CONFIGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS configs (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

pub const ADD_AGENT_TYPE_COLUMN: &str = r#"
ALTER TABLE audit_logs ADD COLUMN agent_type TEXT;
"#;

pub const ADD_PAYLOAD_COLUMN: &str = r#"
ALTER TABLE audit_logs ADD COLUMN payload TEXT;
"#;

pub const CREATE_MCP_SERVERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS mcp_servers (
    name TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    args TEXT NOT NULL DEFAULT '[]',
    env TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

pub const CREATE_REPORTER_QUEUE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS reporter_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    audit_log_rowid INTEGER NOT NULL,
    payload TEXT NOT NULL,
    failures INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',
    created_at TEXT NOT NULL,
    next_retry_at TEXT NOT NULL
);
"#;

pub const CREATE_REPORTER_CURSOR_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS reporter_cursor (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub const INIT_REPORTER_CURSOR: &str = r#"
INSERT OR IGNORE INTO reporter_cursor (key, value) VALUES ('last_rowid', '0');
"#;

// ------------------------------------------------------------------
// Desktop app.db schema (used by ai-coding-desktop Tauri application)
// ------------------------------------------------------------------

pub const CREATE_DESKTOP_PROFILES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE,
    email TEXT,
    phone TEXT,
    role TEXT DEFAULT 'user',
    created_at TEXT
);
"#;

pub const CREATE_DESKTOP_CONVERSATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    title TEXT NOT NULL,
    agent TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at TEXT,
    updated_at TEXT
);
"#;

pub const CREATE_DESKTOP_MESSAGES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    steps TEXT,
    is_complete INTEGER DEFAULT 0,
    token_in INTEGER,
    token_out INTEGER,
    duration_ms INTEGER,
    created_at TEXT
);
"#;

pub const CREATE_DESKTOP_TASKS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    conversation_id TEXT,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT
);
"#;

pub const CREATE_DESKTOP_MODIFIED_FILES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS modified_files (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    conversation_id TEXT,
    file_path TEXT NOT NULL,
    change_type TEXT NOT NULL,
    diff TEXT,
    created_at TEXT
);
"#;

pub const CREATE_DESKTOP_SESSION_LOGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS session_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL,
    instance_id TEXT,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    source TEXT NOT NULL,
    message TEXT NOT NULL,
    payload TEXT
);
"#;

// ------------------------------------------------------------------
// Agent-specific per-instance database schema
// ------------------------------------------------------------------

pub const CREATE_AGENT_CONVERSATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    model TEXT,
    created_at TEXT,
    updated_at TEXT
);
"#;

pub const CREATE_AGENT_MESSAGES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    steps TEXT,
    is_complete INTEGER DEFAULT 0,
    token_in INTEGER,
    token_out INTEGER,
    duration_ms INTEGER,
    created_at TEXT
);
"#;

pub const CREATE_AGENT_TASKS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT
);
"#;

pub const CREATE_AGENT_MODIFIED_FILES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS modified_files (
    id TEXT PRIMARY KEY,
    conversation_id TEXT,
    file_path TEXT NOT NULL,
    change_type TEXT NOT NULL,
    diff TEXT,
    created_at TEXT
);
"#;

pub const CREATE_AGENT_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS agent_meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
"#;

pub const ALL_MIGRATIONS: &[&str] = &[
    CREATE_AUDIT_LOGS_TABLE,
    CREATE_SESSIONS_TABLE,
    CREATE_CONFIGS_TABLE,
    ADD_AGENT_TYPE_COLUMN,
    ADD_PAYLOAD_COLUMN,
    CREATE_MCP_SERVERS_TABLE,
    CREATE_REPORTER_QUEUE_TABLE,
    CREATE_REPORTER_CURSOR_TABLE,
    INIT_REPORTER_CURSOR,
];

/// Migrations used by the desktop Tauri application (`app.db`).
pub const DESKTOP_MIGRATIONS: &[&str] = &[
    CREATE_DESKTOP_PROFILES_TABLE,
    CREATE_DESKTOP_CONVERSATIONS_TABLE,
    CREATE_DESKTOP_MESSAGES_TABLE,
    CREATE_DESKTOP_TASKS_TABLE,
    CREATE_DESKTOP_MODIFIED_FILES_TABLE,
    CREATE_DESKTOP_SESSION_LOGS_TABLE,
];

/// Migrations used by each agent instance's private database.
pub const AGENT_MIGRATIONS: &[&str] = &[
    CREATE_AGENT_CONVERSATIONS_TABLE,
    CREATE_AGENT_MESSAGES_TABLE,
    CREATE_AGENT_TASKS_TABLE,
    CREATE_AGENT_MODIFIED_FILES_TABLE,
    CREATE_AGENT_META_TABLE,
];
