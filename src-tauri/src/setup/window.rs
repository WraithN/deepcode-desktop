use tauri::{Manager, PhysicalSize, Size};

const DEFAULT_WINDOW_WIDTH: u32 = 1500;
const DEFAULT_WINDOW_HEIGHT: u32 = 900;
const WINDOW_LAYOUT_RETRY_DELAY_MS: u64 = 500;

#[tauri::command]
pub fn window_control(window: tauri::Window, action: String) -> Result<(), String> {
    log::info!("[window_control] action={}", action);
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "toggle_maximize" => {
            if window.is_maximized().map_err(|e| e.to_string())? {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => window.close().map_err(|e| e.to_string()),
        _ => Err(format!("unknown window action: {}", action)),
    }
}

#[tauri::command]
pub fn hide_system_cursor(_window: tauri::Window) -> Result<(), String> {
    // 所有平台均使用系统鼠标，此命令已弃用
    Ok(())
}

pub fn show_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("[main.rs] Main window not found");
        return Ok(());
    };
    apply_main_window_layout(&window)?;
    schedule_main_window_layout_retry(window);
    Ok(())
}

fn apply_main_window_layout(window: &tauri::WebviewWindow) -> Result<(), String> {
    let window_size = Size::Physical(PhysicalSize::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT));
    log::info!("[main.rs] Main window found, sizing, showing and focusing");
    window.set_min_size(Some(window_size)).map_err(|e| e.to_string())?;
    window.set_size(window_size).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.center().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    log_window_state(window);
    Ok(())
}

fn schedule_main_window_layout_retry(window: tauri::WebviewWindow) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(WINDOW_LAYOUT_RETRY_DELAY_MS));
        let retry_window = window.clone();
        let _ = window.run_on_main_thread(move || {
            if let Err(error) = apply_main_window_layout(&retry_window) {
                log::error!("[main.rs] Failed to retry main window layout: {}", error);
            }
        });
    });
}

fn log_window_state(window: &tauri::WebviewWindow) {
    let visible = window.is_visible();
    let outer_size = window.outer_size();
    let outer_position = window.outer_position();
    log::info!(
        "[main.rs] Main window state visible={:?}, size={:?}, position={:?}",
        visible,
        outer_size,
        outer_position,
    );
}
