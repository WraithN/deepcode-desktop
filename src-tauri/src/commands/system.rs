use std::net::SocketAddr;
use std::sync::Mutex;
use tauri::State;

pub struct WebSocketState {
    pub addr: Mutex<Option<SocketAddr>>,
}

#[tauri::command]
pub fn get_websocket_url(state: State<'_, WebSocketState>) -> Result<String, String> {
    let addr = state.addr.lock().map_err(|e| e.to_string())?;
    match *addr {
        Some(addr) => Ok(format!("ws://{}", addr)),
        None => Err("WebSocket server not started".to_string()),
    }
}
