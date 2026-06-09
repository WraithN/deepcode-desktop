use std::process::{Command, Stdio};
use tracing::{error, info};

pub struct ProcessManager;

impl ProcessManager {
    pub fn spawn_agent(
        command: &str,
        args: &[String],
        env_vars: &std::collections::HashMap<String, String>,
    ) -> Result<std::process::Child, anyhow::Error> {
        info!("Spawning agent: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let child = cmd.spawn()?;
        info!("Agent spawned with PID: {}", child.id());

        Ok(child)
    }
}
