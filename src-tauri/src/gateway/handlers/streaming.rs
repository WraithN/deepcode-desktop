use crate::gateway::session_manager::SessionManager;
use crate::service::opencode_service::OpencodeService;
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use std::process::Stdio;
use tokio_tungstenite::tungstenite::Message;

/// 流式读取 opencode run 输出并推送到会话 WebSocket
/// 
/// 事件类型：
/// - agent.thinking: AI 开始思考
/// - agent.token: 文本 token（逐字推送）
/// - agent.done: 完成（包含 sessionID）
/// - agent.error: 错误
pub async fn stream_opencode_output(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    conversation_id: String,
    message: String,
    opencode_session_id: Option<String>,
) {
    let attach_url = opencode_service.get_attach_url();

    log::info!(
        "[streaming] starting stream for conversation={}, session={:?}",
        conversation_id, opencode_session_id
    );

    let mut cmd = tokio::process::Command::new("opencode");
    cmd.arg("run")
        .arg(&message)
        .arg("--format")
        .arg("json")
        .arg("--attach")
        .arg(&attach_url);

    if let Some(sid) = &opencode_session_id {
        cmd.arg("--session").arg(sid);
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[streaming] Failed to spawn opencode run: {}", e);
            let _ = send_error(
                &session_manager,
                &conversation_id,
                &format!("Failed to spawn opencode run: {}", e),
            ).await;
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            log::error!("[streaming] Failed to capture stdout");
            let _ = send_error(&session_manager, &conversation_id, "Failed to capture stdout").await;
            return;
        }
    };

    let mut reader = BufReader::new(stdout).lines();
    let mut session_id_result = String::new();

    // 发送 thinking 事件
    let _ = send_event(
        &session_manager,
        &conversation_id,
        "agent.thinking",
        json!({ "content": "AI 正在思考..." }),
    ).await;

    // 逐行读取并推送
    while let Ok(Some(line)) = reader.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        log::debug!("[streaming] received line: {}", line);

        if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
            // 提取 session ID
            if session_id_result.is_empty() {
                if let Some(sid) = event.get("sessionID").and_then(|v| v.as_str()) {
                    session_id_result = sid.to_string();
                }
            }

            // 解析事件类型
            let event_type = event.get("type").and_then(|v| v.as_str());
            let method = match event_type {
                Some("step_start") => "agent.thinking",
                Some("text") => "agent.token",
                Some("step_finish") => "agent.done",
                _ => {
                    log::debug!("[streaming] unknown event type: {:?}", event_type);
                    continue;
                }
            };

            let payload = if method == "agent.token" {
                // 提取文本内容
                let text = event
                    .get("part")
                    .and_then(|p| p.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                json!({ "text": text })
            } else {
                event.get("part").cloned().unwrap_or(event)
            };

            let _ = send_event(&session_manager, &conversation_id, method, payload).await;
        } else {
            log::warn!("[streaming] failed to parse JSON line: {}", line);
        }
    }

    // 检查 stderr
    let stderr_output = {
        let mut stderr_buf = String::new();
        if let Some(stderr) = child.stderr.take() {
            let mut stderr_reader = BufReader::new(stderr);
            let _ = stderr_reader.read_to_string(&mut stderr_buf).await;
        }
        stderr_buf
    };

    // 等待进程结束
    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            log::error!("[streaming] failed to wait for child: {}", e);
            let _ = send_error(&session_manager, &conversation_id, &format!("Process error: {}", e)).await;
            return;
        }
    };

    if !status.success() && !stderr_output.is_empty() {
        log::error!("[streaming] opencode run stderr: {}", stderr_output);
    }

    // 发送 done 事件
    let _ = send_event(
        &session_manager,
        &conversation_id,
        "agent.done",
        json!({ "sessionID": session_id_result }),
    ).await;

    log::info!(
        "[streaming] stream completed for conversation={}, sessionID={}",
        conversation_id, session_id_result
    );
}

async fn send_event(
    session_manager: &SessionManager,
    conversation_id: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<(), String> {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });

    log::debug!("[streaming] sending {} to {}", method, conversation_id);

    session_manager
        .send_to_session(
            conversation_id,
            Message::Text(notification.to_string()),
        )
        .await
}

async fn send_error(
    session_manager: &SessionManager,
    conversation_id: &str,
    message: &str,
) -> Result<(), String> {
    send_event(
        session_manager,
        conversation_id,
        "agent.error",
        json!({ "message": message }),
    )
    .await
}
