//! `StubNotifier` — captures events for test assertions.

use crate::error::NotifyError;
use crate::event::NotificationEvent;
use crate::notifier::Notifier;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct StubNotifier {
    captured: Arc<Mutex<Vec<NotificationEvent>>>,
    /// When `true`, `notify` returns `Err(NotifyError::EndpointError)`
    /// to let tests verify error-path behaviour.
    fail_next: Arc<Mutex<bool>>,
}

impl StubNotifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a snapshot of all captured events without resetting.
    pub fn snapshot(&self) -> Vec<NotificationEvent> {
        self.captured.lock().map(|v| v.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut v) = self.captured.lock() {
            v.clear();
        }
    }

    pub fn arm_failure(&self) {
        if let Ok(mut f) = self.fail_next.lock() {
            *f = true;
        }
    }
}

#[async_trait]
impl Notifier for StubNotifier {
    async fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        let armed = self
            .fail_next
            .lock()
            .map(|mut f| {
                let was = *f;
                *f = false;
                was
            })
            .unwrap_or(false);
        if armed {
            return Err(NotifyError::EndpointError {
                status: 500,
                body_excerpt: "stub failure armed".to_string(),
            });
        }
        if let Ok(mut v) = self.captured.lock() {
            v.push(event.clone());
        }
        Ok(())
    }
}
