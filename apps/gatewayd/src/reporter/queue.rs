use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::interval;

use super::config::ReporterConfig;
use super::exporter::{AuditLogExporter, ExportError};
use dh_db::DbManager;

pub struct RetryWorker {
    db: Arc<std::sync::Mutex<DbManager>>,
    config: ReporterConfig,
    exporter: AuditLogExporter,
}

impl RetryWorker {
    pub fn new(
        db: Arc<std::sync::Mutex<DbManager>>,
        config: ReporterConfig,
        exporter: AuditLogExporter,
    ) -> Self {
        Self {
            db,
            config,
            exporter,
        }
    }

    pub async fn run(&self, mut shutdown: mpsc::Receiver<()>) {
        let mut ticker = interval(tokio::time::Duration::from_secs(10));

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.process_pending().await {
                        eprintln!("[reporter] retry worker error: {}", e);
                    }
                }
                _ = shutdown.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_pending(&self) -> Result<(), Box<dyn std::error::Error>> {
        let now = Utc::now().to_rfc3339();
        let items = {
            let db = self.db.lock().unwrap();
            db.fetch_pending_queue_items(&now, 50)?
        };

        for item in items {
            let payload: serde_json::Value = match serde_json::from_str(&item.payload) {
                Ok(v) => v,
                Err(_) => {
                    let mut db = self.db.lock().unwrap();
                    db.delete_queue_item(item.id)?;
                    continue;
                }
            };

            match self.exporter.export(payload).await {
                Ok(()) => {
                    let mut db = self.db.lock().unwrap();
                    db.delete_queue_item(item.id)?;
                }
                Err(ExportError::ClientError(code, _)) => {
                    let mut db = self.db.lock().unwrap();
                    db.mark_queue_item_dead(item.id, item.failures + 1)?;
                    eprintln!("[reporter] dead letter ({}): queue item {}", code, item.id);
                }
                Err(_) => {
                    let failures = item.failures + 1;
                    if failures as u32 >= self.config.max_retries {
                        let mut db = self.db.lock().unwrap();
                        db.mark_queue_item_dead(item.id, failures)?;
                    } else {
                        let next_retry = calc_backoff(failures);
                        let next_retry_at = (Utc::now() + next_retry).to_rfc3339();
                        let mut db = self.db.lock().unwrap();
                        db.update_queue_item_retry(item.id, failures, &next_retry_at)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn calc_backoff(failures: i32) -> Duration {
    let base = 10i64;
    let exp = std::cmp::min(failures as u32, 8);
    let seconds = base * (2i64.pow(exp));
    let capped = std::cmp::min(seconds, 3600);
    Duration::seconds(capped)
}
