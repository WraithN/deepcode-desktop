// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rusqlite::Connection;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{Manager, State, Listener};
use agent_db::{AgentDbManager, agent_db_create_conversation, agent_db_load_conversations, agent_db_create_message, agent_db_load_messages, agent_db_delete_agent};

mod agent_db;
mod commands;
mod setup;

use dh_desktop::{DbState, RouterState, WebSocketShutdown};
use dh_desktop::service::opencode_service::OpencodeService;
use crate::setup::db::{db_path, init_db};
use crate::setup::window::show_main_window;
use crate::commands::db::*;
use crate::commands::workspace::{get_current_dir, list_workspace_tree, read_workspace_file};
use crate::commands::git::{git_status_workspace, git_changed_files};

#[tauri::command]
async fn agent_send_message_direct(
    opencode_service: State<'_, Arc<OpencodeService>>,
    message: String,
    session_id: Option<String>,
) -> Result<Value, String> {
    opencode_service.run_message(&message, session_id.as_deref()).await
}

fn start_ws_server(
    mut ws_server: dh_desktop::gateway::server::WebSocketServer,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<SocketAddr, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let result = ws_server.start(shutdown_rx).await.map_err(|e| e.to_string());
            let is_ok = result.is_ok();
            let _ = tx.send(result);
            if is_ok {
                std::future::pending::<()>().await;
            }
        });
    });
    rx.recv().map_err(|e| e.to_string())?
}

fn main() {
    env_logger::init();
    log::info!("[main.rs] Starting dh...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            log::info!("[main.rs] Tauri setup callback started");

            let db_path = db_path(app);
            log::info!("[main.rs] Database path: {:?}", db_path);

            let conn = Connection::open(&db_path).expect("打开数据库失败");
            init_db(&conn).expect("初始化数据库失败");
            app.manage(DbState(Mutex::new(conn)));
            app.manage(AgentDbManager::new());
            log::info!("[main.rs] Database initialized");

            // 初始化 SessionLogger
            let app_handle = app.handle().clone();
            let logger_db_path = db_path.clone();
            let logger_conn = Connection::open(&logger_db_path).expect("打开日志数据库失败");
            let logger = std::sync::Arc::new(agent_core::logger::SessionLogger::new(app_handle, logger_conn));
            app.manage(logger.clone());
            log::info!("[main.rs] SessionLogger initialized");

            // 初始化 AgentService 并注册 opencode plugin
            let app_handle = app.handle().clone();
            let mut agent_service = Arc::new(dh_desktop::service::agent_service::AgentService::new(logger.clone()));
            Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
                app_handle,
                logger.clone(),
            )));
            app.manage(agent_service.clone());
            log::info!("[main.rs] AgentService initialized");

            // 初始化服务和 SessionManager
            let db_conn = Connection::open(&db_path).expect("打开数据库失败");
            let db_service = Arc::new(dh_desktop::service::db_service::DbService::new(Arc::new(Mutex::new(db_conn))));
            let opencode_service = Arc::new(dh_desktop::service::opencode_service::OpencodeService::new().unwrap_or_else(|e| {
                log::warn!("[main.rs] Failed to initialize OpencodeService: {}, using fallback", e);
                dh_desktop::service::opencode_service::OpencodeService::new_fallback()
            }));
            app.manage(opencode_service.clone());

            let (event_tx, _event_rx) = tokio::sync::broadcast::channel::<dh_desktop::service::opencode_service::SseEvent>(1000);
            {
                let svc = opencode_service.as_ref();
                svc.set_event_sender(event_tx);
            }

            // Start SSE listener in background
            let svc_for_sse = opencode_service.clone();
            tauri::async_runtime::spawn(async move {
                svc_for_sse.start_event_listener().await;
            });

            let session_manager = Arc::new(dh_desktop::gateway::session_manager::SessionManager::new());
            log::info!("[main.rs] Services initialized");

            // 初始化 WebSocket server
            let router = Arc::new(dh_desktop::gateway::router::GatewayRouter::new(
                agent_service,
                db_service,
                opencode_service,
                session_manager,
            ));
            let ws_server = dh_desktop::gateway::server::WebSocketServer::new(router.clone());
            let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
            let addr = start_ws_server(ws_server, shutdown_rx).unwrap();
            app.manage(WebSocketShutdown { _sender: shutdown_tx });
            app.manage(commands::system::WebSocketState {
                addr: Mutex::new(Some(addr)),
            });
            log::info!("[main.rs] WebSocket server started on: {:?}", addr);

            // 将 Router 存入 Tauri 状态
            app.manage(RouterState(router.clone()));

            // 监听 Tauri session:log 事件，转发为 WebSocket session.log 通知
            let router_for_events = router.clone();
            app.listen("session:log", move |event| {
                let payload = event.payload();
                if let Ok(mut entry) = serde_json::from_str::<serde_json::Value>(&payload) {
                    // 将 snake_case 字段名转为 camelCase（前端 logStore 使用 camelCase）
                    if let Some(v) = entry.get("conversation_id").cloned() {
                        entry["conversationId"] = v;
                    }
                    if let Some(v) = entry.get("instance_id").cloned() {
                        entry["instanceId"] = v;
                    }
                    let conversation_id = entry.get("conversationId").and_then(|v| v.as_str()).unwrap_or("");
                    if !conversation_id.is_empty() {
                        let notification = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "session.log",
                            "params": entry
                        });
                        let router = router_for_events.clone();
                        let cid = conversation_id.to_string();
                        let msg = tokio_tungstenite::tungstenite::Message::Text(notification.to_string());
                        tauri::async_runtime::spawn(async move {
                            let _ = router.session_manager().send_to_session(&cid, msg).await;
                        });
                    }
                }
            });

            log::info!("[main.rs] Tauri setup completed successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_sign_in,
            db_sign_up,
            db_get_profile,
            db_load_conversations,
            db_create_conversation,
            db_update_conversation,
            db_delete_conversation,
            db_load_messages,
            db_create_message,
            db_load_tasks,
            db_create_task,
            db_load_modified_files,
            db_create_modified_file,
            agent_db_create_conversation,
            agent_db_load_conversations,
            agent_db_create_message,
            agent_db_load_messages,
            agent_db_delete_agent,
            get_current_dir,
            setup::window::window_control,
            setup::window::hide_system_cursor,
            list_workspace_tree,
            read_workspace_file,
            git_status_workspace,
            git_changed_files,
            agent_send_message_direct,
            commands::session_log::session_log_load,
            commands::system::get_websocket_url,
            commands::system::get_webview_html,
            commands::system::console_logs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Ready = event {
                if let Err(error) = show_main_window(app) {
                    log::error!("[main.rs] Failed to show main window: {}", error);
                }
            }
        });
}
