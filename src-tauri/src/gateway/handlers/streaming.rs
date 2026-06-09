use crate::gateway::session_manager::SessionManager;
use crate::service::opencode_service::OpencodeService;
use serde_json::json;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

/// 智能分块：将文本拆分为适合流式推送的块
/// - 英文按单词边界拆分
/// - 中文按 1-2 字符拆分
/// - 混合文本智能处理
fn simulate_stream_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut prev_was_ascii = false;

    for ch in text.chars() {
        let is_ascii = ch.is_ascii() && !ch.is_ascii_whitespace();
        let is_space = ch.is_ascii_whitespace();

        if is_space {
            // 空格作为英文单词的结束标记
            if !current.is_empty() {
                current.push(ch);
                if current.len() >= 3 {
                    chunks.push(current.clone());
                    current.clear();
                }
            }
            prev_was_ascii = false;
        } else if is_ascii {
            current.push(ch);
            // 英文单词达到 3-5 个字符时推送，保持自然感
            if prev_was_ascii && current.len() >= 4 {
                chunks.push(current.clone());
                current.clear();
                prev_was_ascii = false;
            } else {
                prev_was_ascii = true;
            }
        } else {
            // 非 ASCII（中文等）：积累 1-2 个字符后推送
            if !current.is_empty() && prev_was_ascii {
                // 切换到中文字符前，先推送积累的英文
                chunks.push(current.clone());
                current.clear();
            }
            current.push(ch);
            if current.chars().count() >= 2 {
                chunks.push(current.clone());
                current.clear();
                prev_was_ascii = false;
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// 通过 opencode serve HTTP API 获取响应，并通过 SSE 事件实现真正的流式推送
///
/// 事件类型：
/// - agent.thinking: AI 开始思考 / 步骤开始
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

    // 订阅 SSE 事件
    let mut sse_rx = match opencode_service.subscribe_events() {
        Ok(rx) => rx,
        Err(e) => {
            log::warn!("[streaming] Failed to subscribe to SSE events: {}, falling back to non-streaming", e);
            // Fallback: wait for complete response then simulate streaming
            stream_fallback(opencode_service, session_manager, conversation_id, message, sid).await;
            return;
        }
    };

    // 启动后台任务消费 SSE 事件并实时转发到 WebSocket
    let session_manager_for_sse = session_manager.clone();
    let conversation_id_for_sse = conversation_id.clone();
    let sid_for_sse = sid.clone();

    let sse_task = tokio::spawn(async move {
        let mut last_status = String::new();
        let mut last_part_texts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut assistant_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut event_count = 0u32;
        let mut sent_any_token = false;

        while let Ok(event) = sse_rx.recv().await {
            event_count += 1;
            log::info!("[streaming SSE] #{} event_type={} session_id={:?}", event_count, event.event_type, event.session_id);

            // 只处理当前 session 的事件
            if event.session_id.as_ref() != Some(&sid_for_sse) {
                log::debug!("[streaming SSE] #{} skipped: session mismatch (expected {})", event_count, sid_for_sse);
                continue;
            }

            match event.event_type.as_str() {
                "message.updated" => {
                    // 跟踪 assistant 消息的 ID，用于过滤用户消息的 part
                    if let Some(props) = event.payload.get("properties") {
                        if let Some(info) = props.get("info") {
                            if let Some(role) = info.get("role").and_then(|v| v.as_str()) {
                                if role == "assistant" {
                                    if let Some(msg_id) = info.get("id").and_then(|v| v.as_str()) {
                                        assistant_message_ids.insert(msg_id.to_string());
                                        log::info!("[streaming SSE] #{} tracked assistant message: {}", event_count, msg_id);
                                    }
                                }
                            }
                        }
                    }
                }
                "message.part.delta" => {
                    // 增量文本更新（真正的流式 token）
                    if let Some(props) = event.payload.get("properties") {
                        let part_id = props.get("partID").and_then(|v| v.as_str()).unwrap_or("");
                        let message_id = props.get("messageID").and_then(|v| v.as_str()).unwrap_or("");
                        // 跳过用户消息的 part，只处理 assistant 消息的 part
                        if !message_id.is_empty() && !assistant_message_ids.contains(message_id) {
                            log::info!("[streaming SSE] #{} delta skipped: message {} is not from assistant", event_count, message_id);
                            continue;
                        }
                        if let Some(delta) = props.get("delta").and_then(|v| v.as_str()) {
                            if !delta.is_empty() {
                                log::info!("[streaming SSE] #{} message.part.delta: msg_id={} part_id={} delta={:?}", event_count, message_id, part_id, &delta[..delta.len().min(50)]);
                                sent_any_token = true;
                                let send_result = send_event(
                                    &session_manager_for_sse,
                                    &conversation_id_for_sse,
                                    "agent.token",
                                    serde_json::json!({ "text": delta }),
                                ).await;
                                log::info!("[streaming SSE] #{} sent agent.token result={:?}", event_count, send_result);
                                // 更新 last_part_texts，避免后续的 message.part.updated 重复发送
                                if !part_id.is_empty() {
                                    let prev = last_part_texts.get(part_id).map(|s| s.as_str()).unwrap_or("");
                                    last_part_texts.insert(part_id.to_string(), format!("{}{}", prev, delta));
                                }
                            }
                        }
                    }
                }
                "thinking" => {
                    log::info!("[streaming SSE] #{} sending agent.thinking with content", event_count);
                    let content = event.payload.get("content").and_then(|v| v.as_str())
                        .or_else(|| event.payload.get("text").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    let send_result = send_event(
                        &session_manager_for_sse,
                        &conversation_id_for_sse,
                        "agent.thinking",
                        serde_json::json!({
                            "content": content,
                            "id": format!("thinking-{}", event_count),
                            "type": "step-start",
                        }),
                    )
                    .await;
                    log::info!("[streaming SSE] #{} sent agent.thinking result={:?} content_len={}", event_count, send_result, content.len());
                }
                "message.part.updated" => {
                    if let Some(props) = event.payload.get("properties") {
                        if let Some(part) = props.get("part") {
                            let part_id = part.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            let part_type = part.get("type").and_then(|v| v.as_str());
                            let message_id = part.get("messageID").and_then(|v| v.as_str()).unwrap_or("");
                            log::info!("[streaming SSE] #{} message.part.updated part_id={} part_type={:?} messageID={}", event_count, part_id, part_type, message_id);

                            // 跳过用户消息的 part，只处理 assistant 消息的 part
                            if !message_id.is_empty() && !assistant_message_ids.contains(message_id) {
                                log::info!("[streaming SSE] #{} skipped: message {} is not from assistant", event_count, message_id);
                                continue;
                            }

                            match part_type {
                                Some("text") => {
                                    // message.part.updated 的 text 已由 message.part.delta 处理，
                                    // 这里只更新 last_part_texts 避免重复，不发送 token
                                    let current_text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                    log::info!("[streaming SSE] #{} text part.updated: skipping token (handled by delta), text_len={}", event_count, current_text.len());
                                    if !part_id.is_empty() {
                                        last_part_texts.insert(part_id.to_string(), current_text.to_string());
                                    }
                                }
                                Some("step-start") => {
                                    log::info!("[streaming SSE] #{} sending agent.thinking", event_count);
                                    let step_text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                    let step_id = part.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                    let send_result = send_event(
                                        &session_manager_for_sse,
                                        &conversation_id_for_sse,
                                        "agent.thinking",
                                        serde_json::json!({
                                            "content": step_text,
                                            "id": step_id,
                                            "type": "step-start",
                                        }),
                                    )
                                    .await;
                                    log::info!("[streaming SSE] #{} sent agent.thinking result={:?}", event_count, send_result);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                "session.status" => {
                    if let Some(props) = event.payload.get("properties") {
                        if let Some(status) = props.get("status")
                            .and_then(|v| v.get("type"))
                            .and_then(|v| v.as_str())
                        {
                            log::info!("[streaming SSE] #{} session.status: {} -> {}", event_count, last_status, status);
                            if status == "idle" && last_status == "busy" {
                                log::info!("[streaming SSE] #{} breaking: busy->idle", event_count);
                                break;
                            }
                            last_status = status.to_string();
                        }
                    }
                }
                "session.idle" => {
                    log::info!("[streaming SSE] #{} breaking: session.idle", event_count);
                    break;
                }
                "session.error" => {
                    log::info!("[streaming SSE] #{} breaking: session.error", event_count);
                    let _ = send_event(
                        &session_manager_for_sse,
                        &conversation_id_for_sse,
                        "agent.error",
                        event.payload,
                    )
                    .await;
                    break;
                }
                _ => {
                    log::info!("[streaming SSE] #{} unhandled event type={}", event_count, event.event_type);
                }
            }
        }
        log::info!("[streaming SSE] consumer exited after {} events, sent_any_token={}", event_count, sent_any_token);
        sent_any_token
    });

    // 调用 opencode serve HTTP API（这会触发后台的 LLM 生成和工具执行）
    log::info!("[streaming] calling send_message for session={}", sid);
    let result = match opencode_service.send_message(&sid, &message).await {
        Ok(result) => {
            log::info!("[streaming] send_message completed, parts count={}", result.get("parts").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0));
            result
        }
        Err(e) => {
            log::error!("[streaming] HTTP API error: {}", e);
            let _ = send_error(&session_manager, &conversation_id, &e).await;
            sse_task.abort();
            return;
        }
    };

    let session_id_result = result
        .get("info")
        .and_then(|i| i.get("sessionID"))
        .and_then(|v| v.as_str())
        .unwrap_or(&sid)
        .to_string();

    // 等待 SSE 消费任务完成（给 SSE 事件一些时间完成推送）
    let sse_sent_tokens = match tokio::time::timeout(tokio::time::Duration::from_secs(10), sse_task).await {
        Ok(Ok(sent)) => {
            log::debug!("[streaming] SSE task completed normally, sent_tokens={}", sent);
            sent
        }
        Ok(Err(e)) => {
            log::warn!("[streaming] SSE task panicked: {}", e);
            false
        }
        Err(_) => {
            log::warn!("[streaming] SSE task timed out, some late events may be dropped");
            false
        }
    };

    // 如果 SSE 已经推送过 token，跳过模拟流式，避免重复发送
    if sse_sent_tokens {
        log::info!("[streaming] SSE already sent tokens, skipping simulated streaming");
    } else {
        log::info!("[streaming] SSE did not send tokens, starting simulated streaming for text parts");
    }

    // 模拟流式：将 text parts 拆分成小块逐个发送（仅当 SSE 未推送 token 时）
    if !sse_sent_tokens {
        log::info!("[streaming] starting simulated streaming for text parts");
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
                    let text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    if !text.is_empty() {
                        // 首字延迟：营造"思考后开始输出"的自然感
                        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

                        // 智能分块：英文按单词边界、中文按字符组
                        let chunks = simulate_stream_chunks(text);
                        let total_chunks = chunks.len().max(1);

                        for (idx, chunk) in chunks.into_iter().enumerate() {
                            let _ = send_event(
                                &session_manager,
                                &conversation_id,
                                "agent.token",
                                serde_json::json!({ "text": chunk }),
                            ).await;

                            // 动态延迟：前 20% 慢（启动感），中间匀速，最后略快
                            let base_delay = if idx < total_chunks / 5 {
                                45u64
                            } else if idx > total_chunks * 4 / 5 {
                                12u64
                            } else {
                                25u64
                            };
                            tokio::time::sleep(tokio::time::Duration::from_millis(base_delay)).await;
                        }
                    }
                }
                Some("step-finish") => {
                    // 步骤完成，不单独推送
                }
                _ => {}
            }
        }
        }
    }

    // 检测 interaction（question / permission / todowrite）
    if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
        let all_parts: Vec<serde_json::Value> = parts.clone();
        if let Some(interaction) = crate::service::opencode_service::detect_interaction_from_parts(&all_parts) {
            let method = match &interaction {
                crate::models::interaction::InteractionRequest::Question { .. } => "agent.question",
                crate::models::interaction::InteractionRequest::Permission { .. } => "agent.permission",
                crate::models::interaction::InteractionRequest::TodoWrite { .. } => "agent.todowrite",
            };
            let interaction_json = match serde_json::to_value(&interaction) {
                Ok(v) => v,
                Err(_) => serde_json::Value::Null,
            };
            let _ = send_event(
                &session_manager,
                &conversation_id,
                method,
                json!({
                    "sessionID": session_id_result,
                    "interaction": interaction_json,
                }),
            ).await;
        }
    }

    // 发送 done 事件
    let done_result = send_event(
        &session_manager,
        &conversation_id,
        "agent.done",
        json!({ "sessionID": session_id_result }),
    )
    .await;
    log::info!("[streaming] sent agent.done result={:?}", done_result);

    log::info!(
        "[streaming] stream completed for conversation={}, sessionID={}",
        conversation_id,
        session_id_result
    );
}

/// Fallback: 当 SSE 订阅失败时，等待完整响应后模拟流式
async fn stream_fallback(
    opencode_service: Arc<OpencodeService>,
    session_manager: Arc<SessionManager>,
    conversation_id: String,
    message: String,
    sid: String,
) {
    match opencode_service.send_message(&sid, &message).await {
        Ok(result) => {
            let session_id_result = result
                .get("info")
                .and_then(|i| i.get("sessionID"))
                .and_then(|v| v.as_str())
                .unwrap_or(&sid)
                .to_string();

            // 模拟流式：逐个 part 发送
            if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
                for part in parts {
                    let part_type = part.get("type").and_then(|v| v.as_str());
                    match part_type {
                        Some("step-start") => {
                            let step_text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                            let step_id = part.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            let _ = send_event(
                                &session_manager,
                                &conversation_id,
                                "agent.thinking",
                                serde_json::json!({
                                    "content": step_text,
                                    "id": step_id,
                                    "type": "step-start",
                                }),
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
                        _ => {}
                    }
                }
            }

            // 检测 interaction
            if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
                let all_parts: Vec<serde_json::Value> = parts.clone();
                if let Some(interaction) = crate::service::opencode_service::detect_interaction_from_parts(&all_parts) {
                    let method = match &interaction {
                        crate::models::interaction::InteractionRequest::Question { .. } => "agent.question",
                        crate::models::interaction::InteractionRequest::Permission { .. } => "agent.permission",
                        crate::models::interaction::InteractionRequest::TodoWrite { .. } => "agent.todowrite",
                    };
                    let interaction_json = match serde_json::to_value(&interaction) {
                        Ok(v) => v,
                        Err(_) => serde_json::Value::Null,
                    };
                    let _ = send_event(
                        &session_manager,
                        &conversation_id,
                        method,
                        json!({
                            "sessionID": session_id_result,
                            "interaction": interaction_json,
                        }),
                    ).await;
                }
            }

            let _ = send_event(
                &session_manager,
                &conversation_id,
                "agent.done",
                json!({ "sessionID": session_id_result }),
            )
            .await;
        }
        Err(e) => {
            log::error!("[streaming] fallback HTTP API error: {}", e);
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
