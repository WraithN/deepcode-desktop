use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};

pub struct ProcessHandle {
    pub pid: u32,
    pub stdout_lines: tokio::io::Lines<BufReader<ChildStdout>>,
    pub kill_tx: Option<oneshot::Sender<()>>,
    pub child: Arc<Mutex<Child>>,
}

pub async fn spawn_command(
    program: &str,
    args: &[String],
    cwd: &str,
) -> Result<ProcessHandle, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {e}"))?;

    let pid = child.id().unwrap_or(0);
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let reader = BufReader::new(stdout);
    let lines = reader.lines();

    let (kill_tx, kill_rx) = oneshot::channel();
    let child_arc = Arc::new(Mutex::new(child));
    let child_clone = Arc::clone(&child_arc);

    tokio::spawn(async move {
        let _ = kill_rx.await;
        let _ = child_clone.lock().await.kill().await;
    });

    Ok(ProcessHandle {
        pid,
        stdout_lines: lines,
        kill_tx: Some(kill_tx),
        child: child_arc,
    })
}

pub async fn kill_process(handle: &mut ProcessHandle) -> Result<(), String> {
    if let Some(tx) = handle.kill_tx.take() {
        let _ = tx.send(());
    }
    handle.child.lock().await.kill().await.map_err(|e| e.to_string())
}
