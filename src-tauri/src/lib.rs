use std::sync::Mutex;

pub mod commands;
pub mod models;
pub mod service;

pub struct DbState(pub Mutex<rusqlite::Connection>);
