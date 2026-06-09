use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotifyError {
    #[error("Notification failed: {0}")]
    Failed(String),
}

pub fn send_notification(title: &str, body: &str) -> Result<(), NotifyError> {
    #[cfg(feature = "notify")]
    {
        notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show()
            .map_err(|e| NotifyError::Failed(e.to_string()))?;
    }

    #[cfg(not(feature = "notify"))]
    {
        tracing::info!("NOTIFICATION: {} - {}", title, body);
    }

    Ok(())
}
