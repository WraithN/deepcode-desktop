use serde::Deserialize;
use std::time::Duration;

fn default_batch_size() -> usize {
    100
}

fn default_flush_interval() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReporterConfig {
    #[serde(default)]
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,
    #[serde(default)]
    pub sanitize_content: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

impl Default for ReporterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            api_key: None,
            batch_size: default_batch_size(),
            flush_interval_secs: default_flush_interval(),
            sanitize_content: false,
            max_retries: default_max_retries(),
        }
    }
}

impl ReporterConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();

        if let Ok(val) = std::env::var("DH_REPORTER_ENABLED") {
            cfg.enabled = val.parse().unwrap_or(false);
        }
        if let Ok(val) = std::env::var("DH_REPORTER_ENDPOINT") {
            cfg.endpoint = Some(val);
        }
        if let Ok(val) = std::env::var("DH_REPORTER_API_KEY") {
            cfg.api_key = Some(val);
        }
        if let Ok(val) = std::env::var("DH_REPORTER_BATCH_SIZE") {
            cfg.batch_size = val.parse().unwrap_or(default_batch_size());
        }
        if let Ok(val) = std::env::var("DH_REPORTER_FLUSH_INTERVAL") {
            cfg.flush_interval_secs = val.parse().unwrap_or(default_flush_interval());
        }
        if let Ok(val) = std::env::var("DH_REPORTER_SANITIZE") {
            cfg.sanitize_content = val.parse().unwrap_or(false);
        }

        cfg
    }

    pub fn flush_interval(&self) -> Duration {
        Duration::from_secs(self.flush_interval_secs)
    }
}
