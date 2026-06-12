use super::types::McpError;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

pub struct StdioTransport {
    stdin: ChildStdin,
    stdout_tx: mpsc::UnboundedSender<String>,
    child: Child,
}

impl StdioTransport {
    pub async fn spawn(command: &str, args: &[String], env: &std::collections::HashMap<String, String>, workspace: &str) -> Result<(Self, mpsc::UnboundedReceiver<String>), McpError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, val) in env {
            cmd.env(key, val);
        }
        
        let mut child = cmd.spawn().map_err(|e| McpError::ProcessError(e.to_string()))?;
        
        let stdin = child.stdin.take().ok_or_else(|| McpError::ProcessError("Failed to open stdin".to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| McpError::ProcessError("Failed to open stdout".to_string()))?;
        
        let (stdout_tx, stdout_rx) = mpsc::unbounded_channel::<String>();
        let stdout_tx_clone = stdout_tx.clone();
        
        // Spawn stdout reader task
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            
            while let Ok(Some(line)) = lines.next_line().await {
                if stdout_tx_clone.send(line).is_err() {
                    break;
                }
            }
        });
        
        // Spawn stderr reader task for logging
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                
                while let Ok(Some(line)) = lines.next_line().await {
                    log::warn!("MCP stderr: {}", line);
                }
            });
        }
        
        Ok((Self {
            stdin,
            stdout_tx,
            child,
        }, stdout_rx))
    }
    
    pub async fn send(&mut self, message: String) -> Result<(), McpError> {
        let json = format!("{}\n", message);
        self.stdin.write_all(json.as_bytes()).await
            .map_err(|e| McpError::ProcessError(e.to_string()))?;
        self.stdin.flush().await
            .map_err(|e| McpError::ProcessError(e.to_string()))?;
        Ok(())
    }
    
    pub fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => false,
        }
    }

    pub fn subscribe(&self) -> mpsc::UnboundedSender<String> {
        self.stdout_tx.clone()
    }
}
