use clap::Args;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::io::Write;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info};

const ADMIN_PORTS: [u16; 5] = [2346, 2347, 2348, 2349, 2350];
const HEALTH_TIMEOUT_SECS: u64 = 1;
const STARTUP_WAIT_ATTEMPTS: usize = 40;
const STARTUP_WAIT_DELAY_MS: u64 = 250;
const WS_SETTLE_DELAY_MS: u64 = 300;
const AGENT_RESPONSE_TIMEOUT_SECS: u64 = 300;
const CREATE_SESSION_TIMEOUT_SECS: u64 = 5;
const CREATE_AGENT_TIMEOUT_SECS: u64 = 10;

const EVENT_TYPE_STATUS_CHANGED: &str = "status_changed";
const EVENT_NAME_AGENT_PERMISSION: &str = "agent.permission";
const EVENT_NAME_AGENT_QUESTION: &str = "agent.question";
const EVENT_NAME_AGENT_TODO_WRITE: &str = "agent.todowrite";

#[derive(Args, Debug)]
pub struct ChatArgs {
    /// Plugin type to chat with (e.g. opencode)
    pub plugin_type: String,

    /// Run in interactive REPL mode
    #[arg(long)]
    pub interactive: bool,
}

/// Identifies a session and the agent instance attached to it.
struct ChatSession {
    session_id: String,
    instance_id: String,
}

pub async fn run(args: ChatArgs) -> Result<(), anyhow::Error> {
    if !args.interactive {
        anyhow::bail!("--interactive is required for now");
    }

    let client = reqwest::Client::new();
    let admin_port = ensure_gatewayd_running(&client).await?;
    let base_url = format!("http://127.0.0.1:{}", admin_port);

    // Create a fresh session and attach the requested plugin.
    let ChatSession {
        session_id,
        instance_id,
    } = create_session_with_agent(&client, &base_url, &args.plugin_type).await?;
    println!(
        "Connected to agent: {} (plugin: {}) in session: {}",
        instance_id, args.plugin_type, session_id
    );
    println!("Type a message and press Enter. Use /quit or /exit to leave.");

    // Establish WebSocket connection to receive and send AG-UI events.
    let ws_url = format!(
        "ws://127.0.0.1:{}/sessions/{}/events",
        admin_port, session_id
    );
    let (ws_stream, _) = connect_async(&ws_url).await?;
    tokio::time::sleep(Duration::from_millis(WS_SETTLE_DELAY_MS)).await;

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Forward incoming WebSocket messages to an unbounded channel so the REPL
    // loop can consume them with a timeout.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<
        Result<Message, tokio_tungstenite::tungstenite::Error>,
    >();
    tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            if event_tx.send(msg).is_err() {
                break;
            }
        }
    });

    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin);
    let mut buf = Vec::new();

    let mut output_state = ReplOutputState { ai_started: false };
    loop {
        if output_state.ai_started {
            println!();
            output_state.ai_started = false;
        }
        print!("[you]>>>> ");
        let _ = std::io::stdout().flush();
        buf.clear();
        match tokio::io::AsyncBufReadExt::read_until(&mut reader, b'\n', &mut buf).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                error!("Failed to read input: {}", e);
                break;
            }
        }

        let input = String::from_utf8_lossy(&buf).trim().to_string();
        if input.is_empty() {
            continue;
        }
        if input == "/quit" || input == "/exit" {
            break;
        }

        let run_id = format!("cli-chat-{}", uuid::Uuid::new_v4());
        let payload = build_run_agent_input(&session_id, &run_id, &args.plugin_type, &input);

        let payload_text = match serde_json::to_string(&payload) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[error] failed to serialize message: {}", e);
                continue;
            }
        };

        if let Err(e) = ws_tx.send(Message::Text(payload_text.into())).await {
            eprintln!("[error] failed to send message: {}", e);
            continue;
        }
        info!("Message sent to session {} run {}", session_id, run_id);

        wait_for_agent_response(&mut event_rx, &mut output_state).await;
    }

    println!("Goodbye.");
    Ok(())
}

/// Build an AG-UI RunAgentInput payload for the user message.
fn build_run_agent_input(session_id: &str, run_id: &str, plugin_type: &str, input: &str) -> Value {
    serde_json::json!({
        "threadId": session_id,
        "runId": run_id,
        "state": {},
        "messages": [
            {
                "role": "user",
                "id": format!("msg-{}", uuid::Uuid::new_v4()),
                "content": input,
            }
        ],
        "tools": [],
        "context": [],
        "forwardedProps": {},
        "agent_key": plugin_type,
    })
}

async fn wait_for_agent_response(
    event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<
        Result<Message, tokio_tungstenite::tungstenite::Error>,
    >,
    output_state: &mut ReplOutputState,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(AGENT_RESPONSE_TIMEOUT_SECS);
    loop {
        let timeout = tokio::time::sleep_until(deadline);
        tokio::pin!(timeout);
        match tokio::select! {
            msg = event_rx.recv() => msg,
            _ = timeout => None,
        } {
            Some(Ok(Message::Text(text))) => {
                if let Ok(event) = serde_json::from_str::<Value>(&text) {
                    if print_event(&event, output_state) {
                        break;
                    }
                }
            }
            Some(Ok(Message::Close(_))) => {
                eprintln!("\n[agent disconnected]");
                break;
            }
            Some(Err(e)) => {
                eprintln!("[ws] error: {}", e);
                break;
            }
            Some(Ok(_)) => {}
            None => break,
        }
    }
}

async fn ensure_gatewayd_running(client: &reqwest::Client) -> Result<u16, anyhow::Error> {
    if let Ok(port) = find_admin_port(client).await {
        return Ok(port);
    }

    println!("dh-gatewayd is not running; starting it now...");
    crate::commands::exec::start_gatewayd().await?;
    wait_for_admin_port(client).await
}

async fn wait_for_admin_port(client: &reqwest::Client) -> Result<u16, anyhow::Error> {
    for _ in 0..STARTUP_WAIT_ATTEMPTS {
        if let Ok(port) = find_admin_port(client).await {
            return Ok(port);
        }
        tokio::time::sleep(Duration::from_millis(STARTUP_WAIT_DELAY_MS)).await;
    }

    anyhow::bail!("dh-gatewayd did not become ready after startup")
}

async fn find_admin_port(client: &reqwest::Client) -> Result<u16, anyhow::Error> {
    for port in ADMIN_PORTS {
        let url = format!("http://127.0.0.1:{}/health", port);
        if is_healthy(client, &url).await {
            return Ok(port);
        }
    }
    anyhow::bail!("dh-gatewayd is not running on any known admin port")
}

async fn is_healthy(client: &reqwest::Client, url: &str) -> bool {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(HEALTH_TIMEOUT_SECS))
        .send()
        .await;
    response
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

/// Create a new session and attach a single agent instance for the plugin.
async fn create_session_with_agent(
    client: &reqwest::Client,
    base_url: &str,
    plugin_type: &str,
) -> Result<ChatSession, anyhow::Error> {
    let session_id = create_session(client, base_url).await?;
    let instance_id = create_agent(client, base_url, &session_id, plugin_type).await?;
    Ok(ChatSession {
        session_id,
        instance_id,
    })
}

async fn create_session(client: &reqwest::Client, base_url: &str) -> Result<String, anyhow::Error> {
    let url = format!("{}/sessions", base_url);
    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(CREATE_SESSION_TIMEOUT_SECS))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("failed to create session: {}", resp.text().await?);
    }

    let body: Value = resp.json().await?;
    body.get("sessionId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing sessionId in create response"))
}

async fn create_agent(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    plugin_type: &str,
) -> Result<String, anyhow::Error> {
    let url = format!("{}/sessions/{}/agents", base_url, session_id);
    let work_directory = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let payload = serde_json::json!({
        "agent_key": plugin_type,
        "name": format!("{}-repl", plugin_type),
        "work_directory": work_directory,
    });

    let resp = client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(CREATE_AGENT_TIMEOUT_SECS))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("failed to create agent: {}", resp.text().await?);
    }

    let body: Value = resp.json().await?;
    body.get("instance_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing instance_id in create response"))
}

struct ReplOutputState {
    ai_started: bool,
}

fn print_event(event: &Value, state: &mut ReplOutputState) -> bool {
    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match event_type {
        "TEXT_MESSAGE_START" => {
            reset_ai_line(state);
            print!("[ai]>>>> ");
            state.ai_started = true;
            let _ = std::io::stdout().flush();
        }
        "TEXT_MESSAGE_CONTENT" => {
            if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                if !delta.is_empty() {
                    if !state.ai_started {
                        print!("[ai]>>>> ");
                        state.ai_started = true;
                    }
                    print!("{}", delta);
                    let _ = std::io::stdout().flush();
                }
            }
        }
        "TEXT_MESSAGE_END" => {
            if state.ai_started {
                println!();
                state.ai_started = false;
            }
            return true;
        }
        "THINKING_TEXT_MESSAGE_CONTENT" => {
            if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                if !delta.is_empty() {
                    reset_ai_line(state);
                    println!("===> ai thinking => {}", delta);
                }
            }
        }
        "TOOL_CALL_START" => {
            reset_ai_line(state);
            let name = event
                .get("toolCallName")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("===> tool_use => {}", name);
        }
        "TOOL_CALL_ARGS" => {
            if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                if !delta.is_empty() {
                    println!("===> tool_args => {}", delta);
                }
            }
        }
        "TOOL_CALL_RESULT" => {
            if let Some(content) = event.get("content").and_then(|v| v.as_str()) {
                println!("===> tool_result => {}", content);
            }
        }
        "RUN_ERROR" => {
            reset_ai_line(state);
            let message = event
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            eprintln!("[error]>>>> {}", message);
            return true;
        }
        "CUSTOM" => {
            if let Some(name) = event.get("name").and_then(|v| v.as_str()) {
                match name {
                    EVENT_TYPE_STATUS_CHANGED => {
                        reset_ai_line(state);
                        let status = event
                            .get("value")
                            .and_then(|v| v.get("status"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        println!("[status]>>>> {}", status);
                    }
                    EVENT_NAME_AGENT_PERMISSION
                    | EVENT_NAME_AGENT_QUESTION
                    | EVENT_NAME_AGENT_TODO_WRITE => {
                        reset_ai_line(state);
                        println!(
                            "[{}]>>>> {}",
                            name,
                            event.get("value").unwrap_or(&Value::Null)
                        );
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    false
}

fn reset_ai_line(state: &mut ReplOutputState) {
    if state.ai_started {
        println!();
        state.ai_started = false;
    }
}
