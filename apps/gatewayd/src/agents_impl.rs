use agent_core::logger::SessionLogger;

pub use agent_core::service::AgentService;

pub fn init_agent_service_with_sink(
    event_sink: std::sync::Arc<dyn agent_core::event_sink::EventSink>,
) -> Result<AgentService, anyhow::Error> {
    let data_dir = dh_platform::fs::data_dir()?;
    let db_path = data_dir.join("agent_logs.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    let log_file = data_dir.join("agent.log");
    let logger = std::sync::Arc::new(SessionLogger::new(event_sink.clone(), conn, Some(log_file)));
    let mut agent_service = AgentService::new(logger.clone(), event_sink.clone());
    agent_service.register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
        logger.clone(),
    )));
    agent_service.register_plugin(Box::new(claude_plugin::plugin::ClaudePlugin::new(
        logger.clone(),
    )));
    agent_service.register_plugin(Box::new(codex_plugin::plugin::CodexPlugin::new(logger)));
    Ok(agent_service)
}
