use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use std::sync::{Arc, Mutex};

use crate::DbError;

#[derive(Clone, Debug, serde::Serialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub username: String,
    pub created_at: String,
}

/// Repository for the desktop `app.db` database.
pub struct AppRepository {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl AppRepository {
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { conn }
    }

    // ------------------------------------------------------------------
    // Auth
    // ------------------------------------------------------------------

    pub fn sign_in(&self, username: &str, _password: &str) -> Result<AuthUser, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, username, email, created_at FROM profiles WHERE username = ?1")
            .map_err(|e| e.to_string())?;
        let user = stmt
            .query_row([username], |row| {
                Ok(AuthUser {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    email: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|_| "用户不存在".to_string())?;
        Ok(user)
    }

    pub fn sign_up(&self, username: &str, _password: &str) -> Result<AuthUser, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
        let email = format!("{username}@local.dev");
        let created_at = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO profiles (id, username, email, phone, role, created_at) VALUES (?1, ?2, ?3, NULL, 'user', ?4)",
            params![&id, username, &email, &created_at],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "用户名已存在".to_string()
            } else {
                e.to_string()
            }
        })?;

        Ok(AuthUser {
            id,
            email,
            username: username.to_string(),
            created_at,
        })
    }

    pub fn get_profile(&self, user_id: &str) -> Result<Option<Value>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, username, email, phone, role, created_at FROM profiles WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let row = stmt
            .query_row([user_id], |row| {
                let val = serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "username": row.get::<_, Option<String>>(1)?,
                    "email": row.get::<_, Option<String>>(2)?,
                    "phone": row.get::<_, Option<String>>(3)?,
                    "role": row.get::<_, String>(4)?,
                    "created_at": row.get::<_, String>(5)?,
                });
                Ok(val)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(row)
    }

    // ------------------------------------------------------------------
    // Conversations
    // ------------------------------------------------------------------

    pub fn load_conversations(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, user_id, title, agent, model, created_at, updated_at FROM conversations WHERE user_id = ?1 ORDER BY updated_at DESC LIMIT ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![user_id, limit], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "user_id": row.get::<_, String>(1)?,
                    "title": row.get::<_, String>(2)?,
                    "agent": row.get::<_, String>(3)?,
                    "model": row.get::<_, String>(4)?,
                    "created_at": row.get::<_, String>(5)?,
                    "updated_at": row.get::<_, String>(6)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }

    pub fn create_conversation(&self, data: &Value) -> Result<Value, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
        let now = chrono::Utc::now().to_rfc3339();
        let user_id = data["user_id"].as_str().unwrap_or("");
        let title = data["title"].as_str().unwrap_or("");
        let agent = data["agent"].as_str().unwrap_or("");
        let model = data["model"].as_str().unwrap_or("");
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, agent, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&id, user_id, title, agent, model, &now, &now],
        ).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": id,
            "user_id": user_id,
            "title": title,
            "agent": agent,
            "model": model,
            "created_at": now,
            "updated_at": now,
        }))
    }

    pub fn update_conversation(&self, id: &str, data: &Value) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if let Some(title) = data["title"].as_str() {
            conn.execute(
                "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, chrono::Utc::now().to_rfc3339(), id],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Messages
    // ------------------------------------------------------------------

    pub fn load_messages(&self, conversation_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, conversation_id, role, content, steps, is_complete, token_in, token_out, duration_ms, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC LIMIT ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![conversation_id, limit], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "conversation_id": row.get::<_, String>(1)?,
                    "role": row.get::<_, String>(2)?,
                    "content": row.get::<_, String>(3)?,
                    "steps": row.get::<_, Option<String>>(4)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                    "is_complete": row.get::<_, i32>(5)? == 1,
                    "token_in": row.get::<_, Option<i64>>(6)?,
                    "token_out": row.get::<_, Option<i64>>(7)?,
                    "duration_ms": row.get::<_, Option<i64>>(8)?,
                    "created_at": row.get::<_, String>(9)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }

    pub fn create_message(&self, data: &Value) -> Result<Value, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
        let now = chrono::Utc::now().to_rfc3339();
        let conversation_id = data["conversation_id"].as_str().unwrap_or("");
        let role = data["role"].as_str().unwrap_or("");
        let content = data["content"].as_str().unwrap_or("");
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&id, conversation_id, role, content, &now],
        ).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": id,
            "conversation_id": conversation_id,
            "role": role,
            "content": content,
            "created_at": now,
        }))
    }

    // ------------------------------------------------------------------
    // Tasks
    // ------------------------------------------------------------------

    pub fn load_tasks(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, user_id, conversation_id, title, status, created_at FROM tasks WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![user_id, limit], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "user_id": row.get::<_, String>(1)?,
                    "conversation_id": row.get::<_, Option<String>>(2)?,
                    "title": row.get::<_, String>(3)?,
                    "status": row.get::<_, String>(4)?,
                    "created_at": row.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }

    pub fn create_task(&self, data: &Value) -> Result<Value, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
        let now = chrono::Utc::now().to_rfc3339();
        let user_id = data["user_id"].as_str().unwrap_or("");
        let conversation_id = data["conversation_id"].as_str();
        let title = data["title"].as_str().unwrap_or("");
        let status = data["status"].as_str().unwrap_or("");
        conn.execute(
            "INSERT INTO tasks (id, user_id, conversation_id, title, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&id, user_id, conversation_id, title, status, &now],
        ).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": id,
            "user_id": user_id,
            "conversation_id": conversation_id,
            "title": title,
            "status": status,
            "created_at": now,
        }))
    }

    // ------------------------------------------------------------------
    // Modified Files
    // ------------------------------------------------------------------

    pub fn load_modified_files(&self, user_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, user_id, conversation_id, file_path, change_type, diff, created_at FROM modified_files WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![user_id, limit], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "user_id": row.get::<_, String>(1)?,
                    "conversation_id": row.get::<_, Option<String>>(2)?,
                    "file_path": row.get::<_, String>(3)?,
                    "change_type": row.get::<_, String>(4)?,
                    "diff": row.get::<_, Option<String>>(5)?,
                    "created_at": row.get::<_, String>(6)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }

    pub fn create_modified_file(&self, data: &Value) -> Result<Value, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
        let now = chrono::Utc::now().to_rfc3339();
        let user_id = data["user_id"].as_str().unwrap_or("");
        let conversation_id = data["conversation_id"].as_str();
        let file_path = data["file_path"].as_str().unwrap_or("");
        let change_type = data["change_type"].as_str().unwrap_or("");
        let diff = data["diff"].as_str();
        conn.execute(
            "INSERT INTO modified_files (id, user_id, conversation_id, file_path, change_type, diff, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&id, user_id, conversation_id, file_path, change_type, diff, &now],
        ).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": id,
            "user_id": user_id,
            "conversation_id": conversation_id,
            "file_path": file_path,
            "change_type": change_type,
            "diff": diff,
            "created_at": now,
        }))
    }

    // ------------------------------------------------------------------
    // Session Logs
    // ------------------------------------------------------------------

    pub fn load_session_logs(&self, conversation_id: &str) -> Result<Vec<Value>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, conversation_id, instance_id, timestamp, level, source, message, payload
                 FROM session_logs
                 WHERE conversation_id = ?1
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![conversation_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "conversation_id": row.get::<_, String>(1)?,
                    "instance_id": row.get::<_, Option<String>>(2)?,
                    "timestamp": row.get::<_, String>(3)?,
                    "level": row.get::<_, String>(4)?,
                    "source": row.get::<_, String>(5)?,
                    "message": row.get::<_, String>(6)?,
                    "payload": row.get::<_, Option<String>>(7)?,
                }))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }

    pub fn create_session_log(
        &self,
        conversation_id: &str,
        instance_id: Option<&str>,
        timestamp: &str,
        level: &str,
        source: &str,
        message: &str,
        payload: Option<&str>,
    ) -> Result<i64, DbError> {
        let conn = self.conn.lock().map_err(|e| DbError::Migration(e.to_string()))?;
        conn.execute(
            "INSERT INTO session_logs (conversation_id, instance_id, timestamp, level, source, message, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![conversation_id, instance_id, timestamp, level, source, message, payload],
        )?;
        Ok(conn.last_insert_rowid())
    }
}
