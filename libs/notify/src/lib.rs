// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Terrashift notifications — Stage-2 detached-mode surface.
//!
//! Pattern: terrashift_plan.md §13 (S13 — detached mode + notifications).
//! Source: this is a Terrashift addition; no upstream counterpart (the reference
//! ships a TUI / chat surface, not migration-job notifications).
//!
//! Constitution:
//! - Article IV (loud failure — notifier errors are surfaced to the
//!   `tracing` log; never silently swallow).
//! - Article V (Slack webhooks may carry sensitive run metadata; the
//!   payload builder strips raw secrets before send).
//!
//! ## Stage 2 surface
//!
//! - `Notifier` trait — async `notify(event) -> Result<(), NotifyError>`
//! - `NotificationEvent` enum — what the migration emits
//! - `LogNotifier` — always-available console fallback (uses `tracing`)
//! - `StubNotifier` — captures events in-memory for tests
//! - `SlackNotifier` — REAL Slack Incoming Webhook adapter
//! - `MultiNotifier` — fan-out to N notifiers (e.g., Log + Slack)
//!
//! ## Stage 2+ deferred
//!
//! - `EmailNotifier` (SMTP)
//! - `WebhookNotifier` (generic POST)
//! - `PagerDutyNotifier` (incident escalation)
//!
//! ## Slack setup
//!
//! Operators need an Incoming Webhook URL — created in Slack at
//! `https://api.slack.com/messaging/webhooks` (free workspace plan
//! supports it). The webhook URL goes in the operator profile:
//!
//! ```toml
//! [notifications.slack]
//! webhook_url     = "https://hooks.slack.com/services/T00/B00/abc123"
//! username        = "Terrashift"            # optional
//! icon_emoji      = ":hammer_and_wrench:"   # optional
//! channel         = "#migrations"           # optional override
//! ```
//!
//! At runtime, `SlackNotifier::from_webhook(url)` returns the notifier;
//! pair it with a `LogNotifier` via `MultiNotifier` so even when Slack
//! is down the operator still sees events on the console.

pub mod error;
pub mod event;
pub mod log_notifier;
pub mod multi;
pub mod notifier;
pub mod slack;
pub mod stub;

pub use error::NotifyError;
pub use event::{NotificationEvent, Severity};
pub use log_notifier::LogNotifier;
pub use multi::MultiNotifier;
pub use notifier::Notifier;
pub use slack::{SlackConfig, SlackNotifier};
pub use stub::StubNotifier;
