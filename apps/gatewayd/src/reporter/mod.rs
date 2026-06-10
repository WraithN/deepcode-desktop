pub mod config;
pub mod exporter;
pub mod poller;
pub mod queue;
pub mod transform;

use std::sync::Arc;
use tokio::sync::mpsc;

use dh_db::DbManager;

use config::ReporterConfig;
use exporter::AuditLogExporter;
use poller::Poller;
use queue::RetryWorker;

#[cfg(test)]
mod tests;

pub struct ReporterHandle {
    poller_shutdown: mpsc::Sender<()>,
    retry_shutdown: mpsc::Sender<()>,
}

impl ReporterHandle {
    pub async fn shutdown(self) {
        let _ = self.poller_shutdown.send(()).await;
        let _ = self.retry_shutdown.send(()).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

pub fn start(
    db: Arc<std::sync::Mutex<DbManager>>,
    config: ReporterConfig,
) -> Option<ReporterHandle> {
    if !config.enabled {
        return None;
    }
    if config.endpoint.is_none() {
        eprintln!("[reporter] enabled but no endpoint configured, skipping");
        return None;
    }

    let exporter = AuditLogExporter::new(&config);

    let (poller_tx, poller_rx) = mpsc::channel(1);
    let (retry_tx, retry_rx) = mpsc::channel(1);

    let poller = Poller::new(db.clone(), config.clone(), exporter);
    let retry = RetryWorker::new(db, config, AuditLogExporter::new(&ReporterConfig::default()));

    tokio::spawn(async move {
        poller.run(poller_rx).await;
    });

    tokio::spawn(async move {
        retry.run(retry_rx).await;
    });

    Some(ReporterHandle {
        poller_shutdown: poller_tx,
        retry_shutdown: retry_tx,
    })
}
