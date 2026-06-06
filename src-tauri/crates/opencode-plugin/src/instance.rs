use agent_core::error::InstanceError;
use agent_core::event::AgentEvent;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::mcp::client::McpClient;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri::Emitter;

pub struct OpencodeInstance {
    config: InstanceConfig,
    status: Arc<Mutex<InstanceStatus>>,
    app_handle: AppHandle,
    logger: Arc<SessionLogger>,
    mcp_client: Arc<Mutex<Option<Arc<McpClient>>>>,
}

impl OpencodeInstance {
    pub fn new(config: InstanceConfig, app_handle: AppHandle, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            app_handle,
            logger,
            mcp_client: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn init_mcp(&self) -> Result<(), InstanceError> {
        {
            let guard = self.mcp_client.lock().unwrap();
            if guard.is_some() {
                return Ok(());
            }
        }

        let conversation_id = self.config.session_id.clone().unwrap_or_default();

        self.logger.log(
            &conversation_id,
            LogLevel::Info,
            "opencode-plugin",
            "Initializing MCP client",
            None,
            Some(self.config.id.clone()),
        );

        let mcp_client = McpClient::spawn(
            "opencode",
            &[
                "mcp-server".to_string(),
                "--dir".to_string(),
                self.config.workspace.clone(),
            ],
            &self.config.workspace,
        )
        .await
        .map_err(|e| InstanceError::McpError(e.to_string()))?;

        let mcp_client = Arc::new(mcp_client);

        // Initialize MCP handshake
        mcp_client
            .initialize()
            .await
            .map_err(|e| InstanceError::McpError(e.to_string()))?;

        // Register notification handler
        let app_handle = self.app_handle.clone();
        let logger = self.logger.clone();
        let instance_id = self.config.id.clone();

        mcp_client.on_notification("notifications/message", move |params| {
            if let Some(event) = crate::mcp_adapter::parse_notification_to_event(&params) {
                let event_type = format!("{:?}", std::mem::discriminant(&event));
                logger.log(
                    "",
                    LogLevel::Debug,
                    "opencode-plugin",
                    &format!("MCP event received: {}", event_type),
                    Some(serde_json::json!({"event": format!("{:?}", event)})),
                    Some(instance_id.clone()),
                );

                let payload = serde_json::json!({
                    "instance_id": instance_id,
                    "event": event,
                });
                let _ = app_handle.emit("agent:event", &payload);
            }
        });

        {
            let mut guard = self.mcp_client.lock().unwrap();
            *guard = Some(mcp_client);
        }

        {
            let mut guard = self.status.lock().unwrap();
            *guard = InstanceStatus::Running { pid: 0 };
        }

        self.emit_status(self.status());

        self.logger.log(
            &conversation_id,
            LogLevel::Info,
            "opencode-plugin",
            "MCP client initialized",
            None,
            Some(self.config.id.clone()),
        );

        Ok(())
    }

    #[allow(dead_code)]
    fn emit_event(&self, event: AgentEvent) {
        let payload = serde_json::json!({
            "instance_id": self.config.id,
            "event": event,
        });
        let _ = self.app_handle.emit("agent:event", &payload);
    }

    fn emit_status(&self, status: InstanceStatus) {
        let _ = self.app_handle.emit(
            "agent:status_changed",
            serde_json::json!({
                "instance_id": self.config.id,
                "status": status,
            }),
        );
    }
}

impl AgentInstance for OpencodeInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    fn plugin_key(&self) -> &'static str {
        "opencode"
    }

    fn send_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let message = message.to_string();
        let conversation_id = conversation_id.to_string();

        Box::pin(async move {
            // Lazy-init MCP if not already done
            self.init_mcp().await?;

            let mcp_client = {
                let guard = self.mcp_client.lock().unwrap();
                guard.clone()
            };

            if let Some(ref mcp_client) = mcp_client {
                mcp_client
                    .call_tool(
                        "send_message",
                        serde_json::json!({
                            "conversation_id": conversation_id,
                            "message": message
                        }),
                    )
                    .await
                    .map_err(|e| InstanceError::McpError(e.to_string()))?;

                Ok(())
            } else {
                Err(InstanceError::McpError(
                    "MCP client not initialized".to_string(),
                ))
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            let mcp_client = {
                let guard = self.mcp_client.lock().unwrap();
                guard.clone()
            };

            if let Some(ref mcp_client) = mcp_client {
                let _ = mcp_client.shutdown().await;
            }

            {
                let mut status = self.status.lock().unwrap();
                *status = InstanceStatus::Stopped;
            }
            self.emit_status(InstanceStatus::Stopped);

            Ok(())
        })
    }
}
