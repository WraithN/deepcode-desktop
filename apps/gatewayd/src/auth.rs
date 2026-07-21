//! API key authentication middleware for the OpenAI/Anthropic compatible
//! endpoints and AG-UI session endpoints.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use rusqlite::{Connection, params};
use std::path::Path;

const ENV_API_KEY: &str = "GATEWAYD_API_KEY";
const CONFIG_KEY_API_KEY: &str = "gatewayd_api_key";
const BEARER_PREFIX: &str = "Bearer ";
const API_KEY_BYTES: usize = 32;

/// API key storage that prefers the environment variable, falls back to the
/// database config table, and auto-generates a persistent key if neither is set.
#[derive(Clone, Debug)]
pub struct ApiKeyStore {
    key: Option<String>,
}

impl ApiKeyStore {
    pub fn new(key: Option<String>) -> Self {
        Self { key }
    }

    /// Load an API key from the environment or database. If neither exists,
    /// generate a random key and persist it in the database.
    pub fn load_or_create(db_path: &Path) -> Self {
        if let Ok(key) = std::env::var(ENV_API_KEY) {
            if !key.is_empty() {
                return Self::new(Some(key));
            }
        }

        let conn = match Connection::open(db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[auth] failed to open db for API key: {}", e);
                return Self::new(None);
            }
        };

        if let Ok(Some(key)) = Self::read_from_db(&conn) {
            return Self::new(Some(key));
        }

        let key = Self::generate_key();
        if let Err(e) = Self::write_to_db(&conn, &key) {
            tracing::warn!("[auth] failed to persist generated API key: {}", e);
            return Self::new(None);
        }
        Self::new(Some(key))
    }

    /// Generate a new API key and persist it to the database.
    pub fn rotate(db_path: &Path) -> anyhow::Result<String> {
        let key = Self::generate_key();
        let conn = Connection::open(db_path)?;
        Self::write_to_db(&conn, &key)?;
        Ok(key)
    }

    fn read_from_db(conn: &Connection) -> rusqlite::Result<Option<String>> {
        let mut stmt = conn.prepare("SELECT value FROM configs WHERE key = ?1")?;
        let mut rows = stmt.query(params![CONFIG_KEY_API_KEY])?;
        match rows.next()? {
            Some(row) => {
                let value: String = row.get(0)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn write_to_db(conn: &Connection, key: &str) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO configs (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![CONFIG_KEY_API_KEY, key, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    fn generate_key() -> String {
        let bytes: [u8; API_KEY_BYTES] = rand::random();
        hex::encode(bytes)
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
}

/// Extracts a bearer token from the `Authorization` header.
fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix(BEARER_PREFIX))
}

/// Authentication middleware that rejects requests without a valid API key when
/// an API key is configured in the application state.
///
/// The request is allowed through if:
/// - no API key is configured, or
/// - the request includes an `Authorization: Bearer {key}` header matching the
///   configured key.
pub async fn auth_middleware(
    State(store): State<ApiKeyStore>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    match store.key() {
        None => Ok(next.run(request).await),
        Some(expected) => {
            let token = extract_bearer(request.headers());
            if token == Some(expected) {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-key"),
        );
        assert_eq!(extract_bearer(&headers), Some("secret-key"));
    }

    #[test]
    fn test_extract_bearer_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer(&headers), None);
    }
}
