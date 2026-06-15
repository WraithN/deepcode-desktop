use dh_db::desktop::{AppRepository, AuthUser};
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub struct DbService {
    repository: AppRepository,
}

impl DbService {
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self {
            repository: AppRepository::new(conn),
        }
    }

    pub fn sign_in(&self, username: String, password: String) -> Result<AuthUser, String> {
        self.repository.sign_in(&username, &password)
    }

    pub fn sign_up(&self, username: String, password: String) -> Result<AuthUser, String> {
        self.repository.sign_up(&username, &password)
    }

    pub fn get_profile(&self, user_id: String) -> Result<Option<Value>, String> {
        self.repository.get_profile(&user_id)
    }

    pub fn load_conversations(&self, user_id: String, limit: i64) -> Result<Vec<Value>, String> {
        self.repository.load_conversations(&user_id, limit)
    }

    pub fn create_conversation(&self, data: Value) -> Result<Value, String> {
        self.repository.create_conversation(&data)
    }

    pub fn update_conversation(&self, id: String, data: Value) -> Result<(), String> {
        self.repository.update_conversation(&id, &data)
    }

    pub fn delete_conversation(&self, id: String) -> Result<(), String> {
        self.repository.delete_conversation(&id)
    }

    pub fn load_messages(
        &self,
        conversation_id: String,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        self.repository.load_messages(&conversation_id, limit)
    }

    pub fn create_message(&self, data: Value) -> Result<Value, String> {
        self.repository.create_message(&data)
    }

    pub fn load_tasks(&self, user_id: String, limit: i64) -> Result<Vec<Value>, String> {
        self.repository.load_tasks(&user_id, limit)
    }

    pub fn create_task(&self, data: Value) -> Result<Value, String> {
        self.repository.create_task(&data)
    }

    pub fn load_modified_files(
        &self,
        user_id: String,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        self.repository.load_modified_files(&user_id, limit)
    }

    pub fn create_modified_file(&self, data: Value) -> Result<Value, String> {
        self.repository.create_modified_file(&data)
    }

    pub fn load_session_logs(&self, conversation_id: String) -> Result<Vec<Value>, String> {
        self.repository.load_session_logs(&conversation_id)
    }
}
