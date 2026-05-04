// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `Notifier` trait — single seam every notification target satisfies.

use crate::error::NotifyError;
use crate::event::NotificationEvent;
use async_trait::async_trait;

/// Notification sink. Implementations:
/// - `LogNotifier` — `tracing` console output (always available)
/// - `StubNotifier` — in-memory capture (test-only)
/// - `SlackNotifier` — POST to Slack Incoming Webhook
/// - `MultiNotifier` — fan-out to N notifiers
///
/// `notify` is async because most adapters do network I/O. Errors
/// surface — caller chooses to retry or log-and-continue.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError>;
}
