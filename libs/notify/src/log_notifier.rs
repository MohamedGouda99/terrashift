//! `LogNotifier` — console fallback that always works.
//!
//! Uses `tracing` so it interoperates with the rest of Terrashift's
//! observability stack. Severity maps to `tracing` levels.

use crate::error::NotifyError;
use crate::event::{NotificationEvent, Severity};
use crate::notifier::Notifier;
use async_trait::async_trait;

#[derive(Debug, Default)]
pub struct LogNotifier;

impl LogNotifier {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Notifier for LogNotifier {
    async fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        let fields_str = if event.fields.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = event
                .fields
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            format!(" [{}]", parts.join(" "))
        };

        let line = format!(
            "{} {} ({}){}: {}",
            event.severity.emoji(),
            event.title,
            event.run_id,
            fields_str,
            event.body
        );

        match event.severity {
            Severity::Info => tracing::info!(target: "terrashift::notify", "{line}"),
            Severity::Action => tracing::warn!(target: "terrashift::notify", "{line}"),
            Severity::Success => tracing::info!(target: "terrashift::notify", "{line}"),
            Severity::Warn => tracing::warn!(target: "terrashift::notify", "{line}"),
            Severity::Error => tracing::error!(target: "terrashift::notify", "{line}"),
        }
        Ok(())
    }
}
