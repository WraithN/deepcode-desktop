//! mcp_servers 表 migration 幂等性测试。
//!
//! 覆盖两条路径：
//! 1. 全新 DB：CREATE TABLE IF NOT EXISTS 直接含 transport/url 列，后续 ALTER 被忽略。
//! 2. 旧 DB：先建不含新列的旧表，再跑 ALL_MIGRATIONS，ALTER 补列成功。

use dh_db::DbManager;
use dh_db::schema::ALL_MIGRATIONS;
use rusqlite::Connection;

/// 断言 mcp_servers 表包含 transport 和 url 两列。
fn assert_has_transport_url_columns(conn: &Connection) {
    let mut cols: Vec<String> = conn
        .prepare("PRAGMA table_info(mcp_servers)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    cols.sort();
    assert!(
        cols.iter().any(|c| c == "transport"),
        "transport column missing: {:?}",
        cols
    );
    assert!(
        cols.iter().any(|c| c == "url"),
        "url column missing: {:?}",
        cols
    );
}

#[test]
fn fresh_db_has_transport_and_url_columns() {
    // 全新内存库：CREATE TABLE IF NOT EXISTS 直接建出新列。
    let manager = DbManager::open_in_memory().unwrap();
    assert_has_transport_url_columns(manager.conn());
}

#[test]
fn old_db_gets_columns_via_alter() {
    // 模拟旧库：先手动建一个不含 transport/url 的旧 mcp_servers 表。
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE mcp_servers (
            name TEXT PRIMARY KEY,
            command TEXT NOT NULL,
            args TEXT NOT NULL DEFAULT '[]',
            env TEXT NOT NULL DEFAULT '{}',
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS configs (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            agent_type TEXT NOT NULL,
            model TEXT NOT NULL,
            workspace TEXT,
            started_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('active', 'idle', 'closed'))
        );
        CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            direction TEXT NOT NULL CHECK(direction IN ('request', 'response')),
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            metadata TEXT NOT NULL DEFAULT '{}'
        );
        CREATE TABLE IF NOT EXISTS reporter_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            audit_log_rowid INTEGER NOT NULL,
            payload TEXT NOT NULL,
            failures INTEGER DEFAULT 0,
            status TEXT DEFAULT 'pending',
            created_at TEXT NOT NULL,
            next_retry_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS reporter_cursor (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .unwrap();

    // 对旧库逐条跑 ALL_MIGRATIONS（复用 DbManager::migrate 的幂等逻辑）。
    // DbManager::open 会重新跑 ALL_MIGRATIONS，但需要先落盘到一个临时文件。
    // 这里直接复用 connection.rs 的 migrate 逻辑：通过 open_in_memory 无法注入旧 schema，
    // 因此改为手动执行 ALL_MIGRATIONS 并复用同样的 ALTER 幂等处理。
    for migration in ALL_MIGRATIONS {
        if migration.contains("ALTER TABLE") && migration.contains("ADD COLUMN") {
            // 幂等 ALTER：列已存在则忽略 "duplicate column name" 错误。
            if let Err(e) = conn.execute_batch(migration) {
                let msg = e.to_string().to_lowercase();
                assert!(
                    msg.contains("duplicate column name") || msg.contains("already exists"),
                    "unexpected ALTER error: {}",
                    e
                );
            }
        } else {
            conn.execute_batch(migration).unwrap();
        }
    }

    assert_has_transport_url_columns(&conn);
}

#[test]
fn migrations_are_idempotent_on_second_run() {
    // 跑两次 ALL_MIGRATIONS，第二次不应报错（CREATE IF NOT EXISTS + ALTER 幂等）。
    let manager = DbManager::open_in_memory().unwrap();
    // 再次手动执行 ALL_MIGRATIONS，模拟应用重启后再次 migrate。
    for migration in ALL_MIGRATIONS {
        if migration.contains("ALTER TABLE") && migration.contains("ADD COLUMN") {
            let _ = manager.conn().execute_batch(migration);
        } else {
            manager.conn().execute_batch(migration).unwrap();
        }
    }
    assert_has_transport_url_columns(manager.conn());
}
