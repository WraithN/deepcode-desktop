use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, Duration};

use super::config::ReporterConfig;
use super::exporter::{AuditLogExporter, ExportError};
use super::transform::{build_otlp_request, transform_audit_log};
use dh_db::DbManager;

pub struct Poller {
    db: Arc<std::sync::Mutex<DbManager>>,
    config: ReporterConfig,
    exporter: AuditLogExporter,
}

impl Poller {
    pub fn new(
        db: Arc<std::sync::Mutex<DbManager>>,
        config: ReporterConfig,
        exporter: AuditLogExporter,
    ) -> Self {
        Self { db, config, exporter }
    }

    pub async fn run(&self, mut shutdown: mpsc::Receiver<()>) {
        let mut ticker = interval(self.config.flush_interval());
        let mut batch = Vec::new();
        let mut last_rowids = Vec::new();

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if !batch.is_empty() {
                        if let Err(e) = self.flush(&mut batch, &mut last_rowids).await {
                            eprintln!("[reporter] flush error: {}", e);
                        }
                    }
                }
                _ = self.poll_once(&mut batch, &mut last_rowids) => {
                    if batch.len() >= self.config.batch_size {
                        if let Err(e) = self.flush(&mut batch, &mut last_rowids).await {
                            eprintln!("[reporter] flush error: {}", e);
                        }
                    }
                }
                _ = shutdown.recv() => {
                    if !batch.is_empty() {
                        if let Err(e) = self.flush(&mut batch, &mut last_rowids).await {
                            eprintln!("[reporter] final flush error: {}", e);
                        }
                    }
                    break;
                }
            }
        }
    }

    async fn poll_once(&self, batch: &mut Vec<serde_json::Value>, last_rowids: &mut Vec<i64>) {
        let (last_rowid, logs) = {
            let db = self.db.lock().unwrap();
            let cursor = match db.get_reporter_cursor() {
                Ok(c) => c,
                Err(_) => return,
            };
            match db.fetch_audit_logs_after(cursor, self.config.batch_size) {
                Ok(rows) => (cursor, rows),
                Err(_) => return,
            }
        };

        for row in logs {
            let record = transform_audit_log(&row, self.config.sanitize_content);
            last_rowids.push(row.rowid);
            batch.push(record);
        }

        if !last_rowids.is_empty() {
            let max_rowid = *last_rowids.iter().max().unwrap_or(&last_rowid);
            let mut db = self.db.lock().unwrap();
            let _ = db.set_reporter_cursor(max_rowid);
        }

        sleep(Duration::from_millis(100)).await;
    }

    async fn flush(
        &self,
        batch: &mut Vec<serde_json::Value>,
        last_rowids: &mut Vec<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if batch.is_empty() {
            return Ok(());
        }

        let request = build_otlp_request(batch.clone());

        match self.exporter.export(request).await {
            Ok(()) => {
                batch.clear();
                last_rowids.clear();
                Ok(())
            }
            Err(ExportError::ClientError(code, _)) => {
                self.enqueue_batch(batch, last_rowids)?;
                batch.clear();
                last_rowids.clear();
                eprintln!("[reporter] client error {}, enqueued to dead letter", code);
                Ok(())
            }
            Err(_) => {
                self.enqueue_batch(batch, last_rowids)?;
                batch.clear();
                last_rowids.clear();
                Ok(())
            }
        }
    }

    fn enqueue_batch(
        &self,
        batch: &[serde_json::Value],
        rowids: &[i64],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payload = serde_json::to_string(&build_otlp_request(batch.to_vec()))?;
        let rowid = rowids.first().copied().unwrap_or(0);
        let mut db = self.db.lock().unwrap();
        db.enqueue_reporter_item(rowid, &payload)?;
        Ok(())
    }
}
