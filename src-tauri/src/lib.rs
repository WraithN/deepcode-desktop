use std::sync::Mutex;

pub mod commands;
pub mod event_sink;
pub mod gateway;
pub mod models;
pub mod service;

pub struct DbState(pub Mutex<rusqlite::Connection>);

pub struct WebSocketShutdown {
    pub _sender: tokio::sync::broadcast::Sender<()>,
}

pub struct RouterState(pub std::sync::Arc<crate::gateway::router::GatewayRouter>);
