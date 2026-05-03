//! `MultiNotifier` — fan-out to multiple notifiers.
//!
//! Strategy: try every child in registration order. Errors on individual
//! children are logged via `tracing` but do **not** abort other children
//! — losing Slack should never silence the LogNotifier console fallback.

use crate::error::NotifyError;
use crate::event::NotificationEvent;
use crate::notifier::Notifier;
use async_trait::async_trait;

pub struct MultiNotifier {
    children: Vec<Box<dyn Notifier>>,
}

impl MultiNotifier {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn with(mut self, notifier: Box<dyn Notifier>) -> Self {
        self.children.push(notifier);
        self
    }

    pub fn add(&mut self, notifier: Box<dyn Notifier>) {
        self.children.push(notifier);
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
}

impl Default for MultiNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Notifier for MultiNotifier {
    /// Fan out to every child. Returns `Ok` if at least one succeeded;
    /// `Err` only when *all* children failed (so the caller knows the
    /// event was actually lost).
    async fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        if self.children.is_empty() {
            return Ok(());
        }

        let mut last_err: Option<NotifyError> = None;
        let mut any_succeeded = false;

        for (idx, child) in self.children.iter().enumerate() {
            match child.notify(event).await {
                Ok(()) => {
                    any_succeeded = true;
                }
                Err(err) => {
                    tracing::warn!(
                        target: "terrashift::notify",
                        child_index = idx,
                        error = %err,
                        "child notifier failed; continuing fan-out"
                    );
                    last_err = Some(err);
                }
            }
        }

        if any_succeeded {
            Ok(())
        } else {
            // All children failed — surface the last error so the
            // caller knows the event was lost everywhere.
            Err(last_err.unwrap_or(NotifyError::EndpointError {
                status: 0,
                body_excerpt: "all child notifiers failed".to_string(),
            }))
        }
    }
}
