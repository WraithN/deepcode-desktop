use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub struct DbManager {
    conn: Connection,
}

impl DbManager {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        let mut manager = Self { conn };
        manager.migrate()?;
        Ok(manager)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let mut manager = Self { conn };
        manager.migrate()?;
        Ok(manager)
    }

    fn migrate(&mut self) -> Result<(), DbError> {
        for migration in crate::schema::ALL_MIGRATIONS {
            self.conn.execute_batch(migration).map_err(|e| {
                DbError::Migration(format!("Failed to run migration: {e}"))
            })?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
