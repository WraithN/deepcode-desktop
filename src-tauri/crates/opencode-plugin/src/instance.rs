use agent_core::error::InstanceError;
use agent_core::event::AgentEvent;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_runtime::process::{kill_process, spawn_command, ProcessHandle};
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
    process_handle: Arc<Mutex<Option<ProcessHandle>>>,
}

impl OpencodeInstance {
    pub fn new(config: InstanceConfig, app_handle: AppHandle, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            app_handle,
            logger,
            process_handle: Arc::new(Mutex::new(None)),
        }
    }

    fn emit_event(&self, event: AgentEvent) {
        let _ = self.app_handle.emit(
            "agent:event",
            serde_json::json!({
                "instance_id": self.config.id,
                "event": event,
            }),
        );
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

    fn send_message(
        &self,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let message = message.to_string();
        Box::pin(async move {
            let conversation_id = self.config.session_id.clone().unwrap_or_default();
            self.logger.log(
                &conversation_id,
                LogLevel::Info,
                "opencode-plugin",
                "send_message called",
                Some(serde_json::json!({ "message": &message, "workspace": &self.config.workspace })),
            );

            let mut args = vec![
                "run".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ];
            if !self.config.workspace.is_empty() {
                args.push("--dir".to_string());
                args.push(self.config.workspace.clone());
            }
            if let Some(ref session) = self.config.session_id {
                args.push("--session".to_string());
                args.push(session.clone());
            }
            args.push(message);

            self.logger.log(
                &conversation_id,
                LogLevel::Debug,
                "opencode-plugin",
                "CLI args built",
                Some(serde_json::json!({ "args": &args })),
            );

            let mut handle = spawn_command("opencode", &args, &self.config.workspace)
                .await
                .map_err(|e| InstanceError::ProcessError(e))?;

            {
                let mut guard = self.status.lock().unwrap();
                *guard = InstanceStatus::Running { pid: handle.pid };
            }
            self.emit_status(self.status());

            self.logger.log(
                &conversation_id,
                LogLevel::Info,
                "agent-runtime",
                "process spawned",
                Some(serde_json::json!({ "pid": handle.pid })),
            );

            let mut events_parsed = 0;
            while let Ok(Some(line)) = handle.stdout_lines.next_line().await {
                if let Some(raw) = crate::parser::parse_opencode_json_line(&line) {
                    let event = crate::mapper::map_to_agent_event(raw);
                    events_parsed += 1;
                    self.emit_event(event);
                }
            }

            if events_parsed == 0 {
                self.emit_event(AgentEvent::TextDelta {
                    content: "(无输出)".to_string(),
                });
            }

            self.emit_event(AgentEvent::Done);

            {
                let mut guard = self.status.lock().unwrap();
                *guard = InstanceStatus::Stopped;
            }
            self.emit_status(InstanceStatus::Stopped);

            self.logger.log(
                &conversation_id,
                LogLevel::Info,
                "opencode-plugin",
                "send_message completed",
                Some(serde_json::json!({ "events_parsed": events_parsed })),
            );

            Ok(())
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            let mut handle = {
                let mut guard = self.process_handle.lock().unwrap();
                guard.take()
            };
            if let Some(ref mut h) = &mut handle {
                kill_process(h).await.map_err(|e| InstanceError::ProcessError(e))?;
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
