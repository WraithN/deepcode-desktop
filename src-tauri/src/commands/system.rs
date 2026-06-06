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

#[tauri::command]
pub async fn get_webview_html(window: tauri::WebviewWindow) -> Result<String, String> {
    let js = r#"
        (function() {
            var html = document.documentElement.outerHTML;
            var logs = [];
            // Capture any error messages in the body
            if (document.body && document.body.innerText) {
                logs.push('Body text: ' + document.body.innerText.substring(0, 500));
            }
            // Check for root element
            var root = document.getElementById('root');
            if (root) {
                logs.push('Root found, children: ' + root.childElementCount);
                logs.push('Root HTML: ' + root.innerHTML.substring(0, 500));
            } else {
                logs.push('Root NOT FOUND');
            }
            return logs.join('\n') + '\n---HTML---\n' + html.substring(0, 2000);
        })()
    "#;
    
    match window.eval(js) {
        Ok(_) => Ok("Eval executed".to_string()),
        Err(e) => Err(format!("Failed to eval JS: {}", e)),
    }
}

#[tauri::command]
pub fn console_logs(logs: Vec<serde_json::Value>) {
    for log in logs {
        if let Some(msg) = log.get("message").and_then(|m| m.as_str()) {
            if let Some(typ) = log.get("type").and_then(|t| t.as_str()) {
                match typ {
                    "error" => log::error!("[JS CONSOLE] {}", msg),
                    _ => log::info!("[JS CONSOLE] {}", msg),
                }
            }
        }
    }
}
