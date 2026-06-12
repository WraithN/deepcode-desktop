use clap::Args;
use futures_util::StreamExt;
use serde_json::Value;
use std::io::Write;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info};

#[derive(Args, Debug)]
pub struct ChatArgs {
    /// Plugin type to chat with (e.g. opencode)
    pub plugin_type: String,

    /// Run in interactive REPL mode
    #[arg(long)]
    pub interactive: bool,
}

pub async fn run(args: ChatArgs) -> Result<(), anyhow::Error> {
    if !args.interactive {
        anyhow::bail!("--interactive is required for now");
    }

    let client = reqwest::Client::new();
    let admin_port = find_admin_port(&client).await?;
    let base_url = format!("http://127.0.0.1:{}", admin_port);
    let ws_url = format!("ws://127.0.0.1:{}/agents/events", admin_port);

    // Find or create an agent instance for the requested plugin type.
    let instance_id = find_or_create_instance(&client, &base_url, &args.plugin_type).await?;
    println!("Connected to agent: {} (plugin: {})", instance_id, args.plugin_type);
    println!("Type a message and press Enter. Use /quit or /exit to leave.");

    // Establish WebSocket connection to receive agent events.
    let (ws_stream, _) = connect_async(format!("{}?instance_id={}", ws_url, instance_id)).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    let (ws_tx, mut ws_rx) = tokio::sync::mpsc::unbounded_channel::<Result<Message, tokio_tungstenite::tungstenite::Error>>();
    tokio::spawn(async move {
        let mut ws_stream = ws_stream;
        while let Some(msg) = ws_stream.next().await {
            if ws_tx.send(msg).is_err() {
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

        let conversation_id = format!("cli-chat-{}", uuid::Uuid::new_v4());
        let url = format!("{}/agents/{}/message", base_url, instance_id);
        let payload = serde_json::json!({
            "conversation_id": conversation_id,
            "message": input,
        });

        match client.post(&url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Message sent to {}", instance_id);
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[error] failed to send message: {} - {}", status, body);
            }
            Err(e) => {
                eprintln!("[error] request failed: {}", e);
            }
        }

        // Drain WebSocket events for up to 10 seconds after sending.
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
        loop {
            let timeout = tokio::time::sleep_until(deadline);
            tokio::pin!(timeout);
            match tokio::select! {
                msg = ws_rx.recv() => msg,
                _ = timeout => None,
            } {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(event) = serde_json::from_str::<Value>(&text) {
                        print_event(&event, &mut output_state);
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

    println!("Goodbye.");
    Ok(())
}

async fn find_admin_port(client: &reqwest::Client) -> Result<u16, anyhow::Error> {
    for port in [2346u16, 2347, 2348, 2349, 2350] {
        let url = format!("http://127.0.0.1:{}/health", port);
        if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(1)).send().await {
            if resp.status().is_success() {
                return Ok(port);
            }
        }
    }
    anyhow::bail!("dh-gatewayd is not running on any known admin port")
}

async fn find_or_create_instance(
    client: &reqwest::Client,
    base_url: &str,
    plugin_type: &str,
) -> Result<String, anyhow::Error> {
    let list_url = format!("{}/agents", base_url);
    let resp = client
        .get(&list_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if resp.status().is_success() {
        let agents: Vec<Value> = resp.json().await?;
        for agent in agents {
            if agent
                .get("plugin_key")
                .and_then(|v| v.as_str())
                .map(|s| s == plugin_type)
                .unwrap_or(false)
            {
                if let Some(id) = agent.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }
    }

    let create_url = format!("{}/agents", base_url);
    let workspace = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let payload = serde_json::json!({
        "plugin_type": plugin_type,
        "name": format!("{}-repl", plugin_type),
        "workspace": workspace,
    });

    let resp = client
        .post(&create_url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10))
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

fn print_event(event: &Value, state: &mut ReplOutputState) {
    let event_type = event
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let payload = event.get("payload").unwrap_or(&Value::Null);

    match event_type {
        "agent:status_changed" => {
            if let Some(status) = payload.get("status") {
                let text = match status {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Object(map) if map.contains_key("running") => "running".to_string(),
                    serde_json::Value::Object(map) if map.contains_key("crashed") => {
                        format!("crashed: {}", map.get("crashed").and_then(|v| v.as_str()).unwrap_or("unknown"))
                    }
                    _ => status.to_string(),
                };
                if state.ai_started {
                    println!();
                    state.ai_started = false;
                }
                println!("[status]>>>> {}", text);
            }
        }
        "agent.thinking" => {
            if let Some(text) = payload.get("content").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    if state.ai_started {
                        println!();
                        state.ai_started = false;
                    }
                    println!("[thinking]>>>> {}", text);
                }
            }
        }
        "agent.token" => {
            if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    if !state.ai_started {
                        print!("[ai]>>>> ");
                        state.ai_started = true;
                    }
                    print!("{}", text);
                    let _ = std::io::stdout().flush();
                }
            }
        }
        "agent.done" => {
            if state.ai_started {
                println!();
                state.ai_started = false;
            }
        }
        "agent.question" | "agent.permission" | "agent.todowrite" => {
            if state.ai_started {
                println!();
                state.ai_started = false;
            }
            println!("[{}]>>>> {}", event_type, payload);
        }
        _ => {}
    }
}
