use chrono::Utc;
use rusqlite::params;

use crate::{DbError, DbManager};

#[derive(Debug, Clone)]
pub struct AuditLogRow {
    pub rowid: i64,
    pub id: String,
    pub session_id: String,
    pub request_id: String,
    pub direction: String,
    pub provider: String,
    pub model: String,
    pub agent_type: Option<String>,
    pub payload: Option<String>,
    pub payload_size_bytes: i64,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub timestamp: String,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct QueueItem {
    pub id: i64,
    pub audit_log_rowid: i64,
    pub payload: String,
    pub failures: i32,
    pub status: String,
    pub created_at: String,
    pub next_retry_at: String,
}

impl DbManager {
    pub fn get_reporter_cursor(&self) -> Result<i64, DbError> {
        let mut stmt = self.conn().prepare(
            "SELECT value FROM reporter_cursor WHERE key = 'last_rowid'"
        )?;
        let rowid: String = stmt.query_row([], |row| row.get(0))?;
        Ok(rowid.parse().unwrap_or(0))
    }

    pub fn set_reporter_cursor(&mut self, rowid: i64) -> Result<(), DbError> {
        self.conn_mut().execute(
            "UPDATE reporter_cursor SET value = ?1 WHERE key = 'last_rowid'",
            params![rowid.to_string()],
        )?;
        Ok(())
    }

    pub fn fetch_audit_logs_after(
        &self,
        last_rowid: i64,
        limit: usize,
    ) -> Result<Vec<AuditLogRow>, DbError> {
        let mut stmt = self.conn().prepare(
            "SELECT rowid, id, session_id, request_id, direction, provider, model,
                    agent_type, payload, payload_size_bytes, prompt_tokens,
                    completion_tokens, total_tokens, timestamp, metadata
             FROM audit_logs
             WHERE rowid > ?1
             ORDER BY rowid
             LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![last_rowid, limit as i64], |row| {
            Ok(AuditLogRow {
                rowid: row.get(0)?,
                id: row.get(1)?,
                session_id: row.get(2)?,
                request_id: row.get(3)?,
                direction: row.get(4)?,
                provider: row.get(5)?,
                model: row.get(6)?,
                agent_type: row.get(7)?,
                payload: row.get(8)?,
                payload_size_bytes: row.get(9)?,
                prompt_tokens: row.get(10)?,
                completion_tokens: row.get(11)?,
                total_tokens: row.get(12)?,
                timestamp: row.get(13)?,
                metadata: row.get(14)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn enqueue_reporter_item(
        &mut self,
        audit_log_rowid: i64,
        payload: &str,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        self.conn_mut().execute(
            "INSERT INTO reporter_queue (audit_log_rowid, payload, created_at, next_retry_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![audit_log_rowid, payload, now],
        )?;
        Ok(())
    }

    pub fn fetch_pending_queue_items(
        &self,
        now: &str,
        limit: usize,
    ) -> Result<Vec<QueueItem>, DbError> {
        let mut stmt = self.conn().prepare(
            "SELECT id, audit_log_rowid, payload, failures, status, created_at, next_retry_at
             FROM reporter_queue
             WHERE status = 'pending' AND next_retry_at <= ?1
             ORDER BY next_retry_at
             LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![now, limit as i64], |row| {
            Ok(QueueItem {
                id: row.get(0)?,
                audit_log_rowid: row.get(1)?,
                payload: row.get(2)?,
                failures: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                next_retry_at: row.get(6)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn update_queue_item_retry(
        &mut self,
        id: i64,
        failures: i32,
        next_retry_at: &str,
    ) -> Result<(), DbError> {
        self.conn_mut().execute(
            "UPDATE reporter_queue SET failures = ?1, next_retry_at = ?2 WHERE id = ?3",
            params![failures, next_retry_at, id],
        )?;
        Ok(())
    }

    pub fn mark_queue_item_dead(
        &mut self,
        id: i64,
        failures: i32,
    ) -> Result<(), DbError> {
        self.conn_mut().execute(
            "UPDATE reporter_queue SET status = 'dead', failures = ?1 WHERE id = ?2",
            params![failures, id],
        )?;
        Ok(())
    }

    pub fn delete_queue_item(&mut self, id: i64) -> Result<(), DbError> {
        self.conn_mut().execute(
            "DELETE FROM reporter_queue WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_queue_stats(&self) -> Result<(i64, i64), DbError> {
        let pending: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM reporter_queue WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        let dead: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM reporter_queue WHERE status = 'dead'",
            [],
            |row| row.get(0),
        )?;
        Ok((pending, dead))
    }
}
