use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub struct ConnectionHandle {
    pub id: String,
    pub sender: tokio::sync::mpsc::UnboundedSender<Message>,
}

impl ConnectionHandle {
    pub fn send(&self, msg: Message) {
        let _ = self.sender.send(msg);
    }
}

/// 会话管理器：管理按 conversation_id 分组的 WebSocket 连接
/// 每个会话可以有多个连接（如多个标签页），但通常只有一个
pub struct SessionManager {
    connections: Arc<RwLock<HashMap<String, Vec<ConnectionHandle>>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册连接到指定会话（同一连接 ID 去重，避免重复推送）
    pub async fn register(&self, conversation_id: String, handle: ConnectionHandle) {
        let mut conns = self.connections.write().await;
        let handles = conns.entry(conversation_id).or_insert_with(Vec::new);
        // 去重：如果已有相同 conn_id，先移除旧 handle
        handles.retain(|h| h.id != handle.id);
        handles.push(handle);
    }

    /// 从指定会话注销连接
    pub async fn unregister(&self, conversation_id: &str, conn_id: &str) {
        let mut conns = self.connections.write().await;
        if let Some(handles) = conns.get_mut(conversation_id) {
            handles.retain(|h| h.id != conn_id);
            if handles.is_empty() {
                conns.remove(conversation_id);
            }
        }
    }

    /// 向指定会话的所有连接发送消息
    pub async fn send_to_session(&self, conversation_id: &str, msg: Message) -> Result<(), String> {
        let conns = self.connections.read().await;
        if let Some(handles) = conns.get(conversation_id) {
            let mut failed = 0;
            for handle in handles {
                if handle.sender.send(msg.clone()).is_err() {
                    failed += 1;
                }
            }
            if failed > 0 {
                log::warn!(
                    "[SessionManager] {} of {} connections failed to receive message for session {}",
                    failed, handles.len(), conversation_id
                );
            }
            Ok(())
        } else {
            Err(format!("Session {} not found", conversation_id))
        }
    }

    /// 检查会话是否活跃
    pub async fn is_session_active(&self, conversation_id: &str) -> bool {
        let conns = self.connections.read().await;
        conns.get(conversation_id).map_or(false, |h| !h.is_empty())
    }

    /// 获取活跃会话数量
    pub async fn active_session_count(&self) -> usize {
        let conns = self.connections.read().await;
        conns.len()
    }

    /// 从所有会话中注销指定连接
    pub async fn unregister_all(&self, conn_id: &str) {
        let mut conns = self.connections.write().await;
        let mut empty_keys = Vec::new();
        for (key, handles) in conns.iter_mut() {
            handles.retain(|h| h.id != conn_id);
            if handles.is_empty() {
                empty_keys.push(key.clone());
            }
        }
        for key in empty_keys {
            conns.remove(&key);
        }
    }
}
