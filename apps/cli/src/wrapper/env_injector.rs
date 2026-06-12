use std::collections::HashMap;

pub fn build_env_map(api_port: u16, proxy_port: u16) -> HashMap<String, String> {
    let mut env = HashMap::new();

    env.insert("OPENAI_BASE_URL".to_string(), format!("http://127.0.0.1:{}/v1", api_port));
    env.insert("ANTHROPIC_BASE_URL".to_string(), format!("http://127.0.0.1:{}/v1", api_port));

    env.insert("HTTP_PROXY".to_string(), format!("http://127.0.0.1:{}", proxy_port));
    env.insert("NO_PROXY".to_string(), "localhost,127.0.0.1,::1".to_string());

    env.insert("DEEPHARNESS_GATEWAYD_PORT".to_string(), api_port.to_string());
    if let Ok(session_id) = std::env::var("DEEPHARNESS_SESSION_ID") {
        env.insert("DEEPHARNESS_SESSION_ID".to_string(), session_id);
    }

    env
}
