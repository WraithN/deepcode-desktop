use std::path::PathBuf;
use tracing::{info, warn};

/// Configuration interceptor that temporarily redirects agent's LLM API calls to gatewayd.
pub struct ConfigInterceptor {
    agent: String,
    gatewayd_url: String,
    backups: Vec<ConfigBackup>,
}

struct ConfigBackup {
    path: PathBuf,
    original_content: String,
}

impl ConfigInterceptor {
    pub fn new(agent: &str, gatewayd_url: &str) -> Self {
        Self {
            agent: agent.to_string(),
            gatewayd_url: gatewayd_url.to_string(),
            backups: Vec::new(),
        }
    }

    /// Intercept the agent's configuration to route LLM requests through gatewayd.
    /// Returns true if interception succeeded.
    pub fn intercept(&mut self) -> anyhow::Result<bool> {
        match self.agent.as_str() {
            "opencode" => self.intercept_opencode(),
            _ => {
                info!("No config interception needed for agent: {}", self.agent);
                Ok(false)
            }
        }
    }

    /// Restore all backed-up configurations.
    pub fn restore(&mut self) {
        for backup in &self.backups {
            if let Err(e) = std::fs::write(&backup.path, &backup.original_content) {
                warn!(
                    "Failed to restore config {}: {}",
                    backup.path.display(),
                    e
                );
            } else {
                info!("Restored config: {}", backup.path.display());
            }
        }
        self.backups.clear();
    }

    fn intercept_opencode(&mut self) -> anyhow::Result<bool> {
        let config_path = dirs::home_dir()
            .map(|h| h.join(".config/opencode/opencode.json"))
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

        if !config_path.exists() {
            info!("opencode config not found at {}", config_path.display());
            return Ok(false);
        }

        let content = std::fs::read_to_string(&config_path)?;
        let mut config: serde_json::Value = serde_json::from_str(&content)?;

        let mut modified = false;

        if let Some(providers) = config.get_mut("provider").and_then(|p| p.as_object_mut()) {
            for (provider_name, provider_config) in providers {
                if let Some(options) = provider_config
                    .get_mut("options")
                    .and_then(|o| o.as_object_mut())
                {
                    if let Some(original_url) = options.get("baseURL") {
                        let original = original_url.as_str().unwrap_or("").to_string();
                        info!(
                            "Redirecting opencode provider '{}' from {} to {}",
                            provider_name, original, self.gatewayd_url
                        );
                        options.insert(
                            "baseURL".to_string(),
                            serde_json::Value::String(self.gatewayd_url.clone()),
                        );
                        modified = true;
                    }
                }
            }
        }

        if modified {
            self.backups.push(ConfigBackup {
                path: config_path.clone(),
                original_content: content,
            });

            let new_content = serde_json::to_string_pretty(&config)?;
            std::fs::write(&config_path, new_content)?;
            info!("Intercepted opencode config at {}", config_path.display());
        }

        Ok(modified)
    }
}

impl Drop for ConfigInterceptor {
    fn drop(&mut self) {
        if !self.backups.is_empty() {
            info!("Restoring agent configurations on exit...");
            self.restore();
        }
    }
}
