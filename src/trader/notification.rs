/// Notification system for trading alerts
pub trait Notifier: Send + Sync {
    fn notify(&self, message: &str);
}

/// Log-based notifier
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, message: &str) {
        tracing::info!("[NOTIFICATION] {}", message);
    }
}
