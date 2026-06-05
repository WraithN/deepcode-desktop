use crate::gateway::session_manager::SessionManager;
use crate::service::opencode_service::OpencodeService;
use serde_json::json;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

/// 通过 opencode serve HTTP API 获取响应，并模拟流式事件推送到会话 WebSocket
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
    log::info!(
        "[streaming] starting stream for conversation={}, session={:?}",
        conversation_id,
        opencode_session_id
    );

    // 创建或复用 session
    let sid = match opencode_session_id {
        Some(sid) if !sid.is_empty() => sid,
        _ => match opencode_service.create_session().await {
            Ok(session) => session
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            Err(e) => {
                log::error!("[streaming] Failed to create session: {}", e);
                let _ = send_error(&session_manager, &conversation_id, &e).await;
                return;
            }
        },
    };

    if sid.is_empty() {
        let _ = send_error(&session_manager, &conversation_id, "Failed to get session ID").await;
        return;
    }

    // 发送 thinking 事件
    let _ = send_event(
        &session_manager,
        &conversation_id,
        "agent.thinking",
        json!({ "content": "AI 正在思考..." }),
    )
    .await;

    // 调用 opencode serve HTTP API
    match opencode_service.send_message(&sid, &message).await {
        Ok(result) => {
            let session_id_result = result
                .get("info")
                .and_then(|i| i.get("sessionID"))
                .and_then(|v| v.as_str())
                .unwrap_or(&sid)
                .to_string();

            // 遍历 parts，模拟流式事件
            if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
                for part in parts {
                    let part_type = part.get("type").and_then(|v| v.as_str());
                    match part_type {
                        Some("step-start") => {
                            let _ = send_event(
                                &session_manager,
                                &conversation_id,
                                "agent.thinking",
                                part.clone(),
                            )
                            .await;
                        }
                        Some("text") => {
                            let _ = send_event(
                                &session_manager,
                                &conversation_id,
                                "agent.token",
                                part.clone(),
                            )
                            .await;
                        }
                        Some("step-finish") => {
                            // 步骤完成，不单独推送
                        }
                        _ => {
                            log::debug!("[streaming] unknown part type: {:?}", part_type);
                        }
                    }
                }
            }

            // 发送 done 事件
            let _ = send_event(
                &session_manager,
                &conversation_id,
                "agent.done",
                json!({ "sessionID": session_id_result }),
            )
            .await;

            log::info!(
                "[streaming] stream completed for conversation={}, sessionID={}",
                conversation_id,
                session_id_result
            );
        }
        Err(e) => {
            log::error!("[streaming] HTTP API error: {}", e);
            let _ = send_error(&session_manager, &conversation_id, &e).await;
        }
    }
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
