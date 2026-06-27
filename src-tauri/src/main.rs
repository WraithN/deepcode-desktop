// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Manager;
use agent_db::{AgentDbManager, agent_db_create_conversation, agent_db_load_conversations, agent_db_create_message, agent_db_load_messages, agent_db_delete_agent};

mod agent_db;
mod commands;
mod setup;

use dh_desktop::{DbState, RouterState, WebSocketShutdown};
use crate::setup::db::db_path;
use crate::setup::window::show_main_window;
use crate::commands::db::*;
use crate::commands::workspace::{get_current_dir, list_workspace_tree, read_workspace_file};
use crate::commands::git::{git_status_workspace, git_changed_files};

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

            let db_manager = dh_db::DbManager::open_desktop(&db_path).expect("打开数据库失败");
            let shared_conn = Arc::new(Mutex::new(db_manager.into_inner()));
            app.manage(DbState(Arc::clone(&shared_conn)));
            app.manage(AgentDbManager::new());
            log::info!("[main.rs] Database initialized");

            // 初始化 SessionManager（提前创建以便 EventSink 使用）
            let session_manager = Arc::new(dh_desktop::gateway::session_manager::SessionManager::new());
            log::info!("[main.rs] SessionManager initialized");

            // 初始化 WebSocket EventSink
            let ws_event_sink = Arc::new(dh_desktop::event_sink::WebSocketEventSink::new(session_manager.clone()));

            // 初始化 SessionLogger（通过 EventSink 解耦 Tauri）
            let logger_db_path = db_path.clone();
            let logger_conn = dh_db::DbManager::open_desktop(&logger_db_path)
                .expect("打开日志数据库失败")
                .into_inner();
            let log_file_path = app.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("session.log");
            let logger = std::sync::Arc::new(agent_core::logger::SessionLogger::new(
                ws_event_sink.clone(),
                logger_conn,
                Some(log_file_path),
            ));
            app.manage(logger.clone());
            log::info!("[main.rs] SessionLogger initialized");

            // 初始化 AgentService 并注册 opencode / claude / codex plugins
            let mut agent_service = Arc::new(dh_desktop::service::agent_service::AgentService::new(logger.clone(), ws_event_sink.clone()));
            Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
                logger.clone(),
            )));
            Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(claude_plugin::plugin::ClaudePlugin::new(
                logger.clone(),
            )));
            Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(codex_plugin::plugin::CodexPlugin::new(
                logger.clone(),
            )));
            app.manage(agent_service.clone());
            log::info!("[main.rs] AgentService initialized");

            // 初始化 DbService（复用同一个数据库连接）
            let db_service = Arc::new(dh_desktop::service::db_service::DbService::new(Arc::clone(&shared_conn)));
            log::info!("[main.rs] Services initialized");

            // 初始化 WebSocket server
            let router = Arc::new(dh_desktop::gateway::router::GatewayRouter::new(
                agent_service,
                db_service,
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

            // SessionLogger now emits directly via WebSocketEventSink,
            // so we no longer need the Tauri -> WebSocket bridge here.
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
