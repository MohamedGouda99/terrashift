// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `SlackNotifier` — Slack Incoming Webhook adapter.
//!
//! Pattern: terrashift_plan.md §13 (S13 — detached mode + notifications).
//! Source: this is a Terrashift addition. Stage-2 ships the real adapter
//! (operator's first-choice channel per S13 prereq).
//!
//! Slack Incoming Webhooks accept JSON via POST. The minimal payload
//! is `{"text": "..."}` but we use **attachments** so each event gets
//! a colored sidebar (severity-keyed) and structured key-value rows.
//! Reference: https://api.slack.com/messaging/webhooks
//!
//! ## Constitution
//!
//! - **Article V**: the webhook URL is itself a credential (anyone with
//!   it can post to your channel). The operator stores it via the
//!   credential broker (`{{secret:slack_webhook}}`); we accept the
//!   resolved URL as a `String` here so this module doesn't depend on
//!   `terrashift_creds`. Callers do the broker resolution upstream.
//! - **Article XIII rule 5**: notification payloads can carry run
//!   metadata; the `NotificationEvent` builder strips raw secrets via
//!   the standard scrubber path before reaching this notifier.

use crate::error::NotifyError;
use crate::event::NotificationEvent;
use crate::notifier::Notifier;
use async_trait::async_trait;
use serde::Serialize;
use std::time::Duration;

/// Configuration for the Slack notifier. `webhook_url` is the only
/// required field; the rest fine-tune presentation.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    pub webhook_url: String,
    /// Override the bot username shown in Slack. Defaults to "Terrashift".
    pub username: Option<String>,
    /// Emoji icon for the bot. Defaults to ":hammer_and_wrench:".
    pub icon_emoji: Option<String>,
    /// Override the channel (must already exist; webhook tokens are
    /// channel-scoped on creation but Slack allows webhook-level
    /// override on Enterprise plans).
    pub channel: Option<String>,
    /// HTTP timeout for the webhook POST. Defaults to 5 seconds — we
    /// don't want notifications blocking the main migration loop.
    pub timeout: Duration,
}

impl SlackConfig {
    pub fn new(webhook_url: impl Into<String>) -> Self {
        Self {
            webhook_url: webhook_url.into(),
            username: None,
            icon_emoji: None,
            channel: None,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_username(mut self, name: impl Into<String>) -> Self {
        self.username = Some(name.into());
        self
    }

    pub fn with_icon_emoji(mut self, emoji: impl Into<String>) -> Self {
        self.icon_emoji = Some(emoji.into());
        self
    }

    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[derive(Debug)]
pub struct SlackNotifier {
    config: SlackConfig,
    client: reqwest::Client,
}

impl SlackNotifier {
    /// Build a notifier from raw config. Validates the webhook URL
    /// shape (must start with `https://hooks.slack.com/`) — fails fast
    /// on a typo'd URL rather than discovering it on the first event.
    pub fn new(config: SlackConfig) -> Result<Self, NotifyError> {
        if !config.webhook_url.starts_with("https://hooks.slack.com/") {
            return Err(NotifyError::InvalidConfig(format!(
                "Slack webhook URL must start with https://hooks.slack.com/ (got: {})",
                first_n_chars(&config.webhook_url, 30)
            )));
        }
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| NotifyError::Network(Box::new(e)))?;
        Ok(Self { config, client })
    }

    /// Convenience constructor — webhook URL only.
    pub fn from_webhook(webhook_url: impl Into<String>) -> Result<Self, NotifyError> {
        Self::new(SlackConfig::new(webhook_url))
    }

    /// Build the Slack JSON payload for an event.
    fn build_payload<'a>(&'a self, event: &'a NotificationEvent) -> SlackPayload<'a> {
        let attachment_fields: Vec<SlackField<'_>> = event
            .fields
            .iter()
            .map(|(k, v)| SlackField {
                title: k,
                value: v,
                short: v.len() <= 32,
            })
            .collect();

        SlackPayload {
            text: format!("{} {}", event.severity.emoji(), event.title),
            username: self.config.username.as_deref(),
            icon_emoji: self.config.icon_emoji.as_deref(),
            channel: self.config.channel.as_deref(),
            attachments: vec![SlackAttachment {
                color: event.severity.slack_color(),
                title: &event.title,
                text: &event.body,
                fields: attachment_fields,
                footer: "terrashift",
                ts: event.timestamp.timestamp(),
            }],
        }
    }
}

#[async_trait]
impl Notifier for SlackNotifier {
    async fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        let payload = self.build_payload(event);
        let body = serde_json::to_string(&payload)?;

        let response = self
            .client
            .post(&self.config.webhook_url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            Err(NotifyError::EndpointError {
                status,
                body_excerpt: first_n_chars(&body, 200).to_string(),
            })
        }
    }
}

// ---------------------------------------------------------------------
// Internal Slack JSON shape
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SlackPayload<'a> {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_emoji: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
    attachments: Vec<SlackAttachment<'a>>,
}

#[derive(Debug, Serialize)]
struct SlackAttachment<'a> {
    color: &'a str,
    title: &'a str,
    text: &'a str,
    fields: Vec<SlackField<'a>>,
    footer: &'a str,
    ts: i64,
}

#[derive(Debug, Serialize)]
struct SlackField<'a> {
    title: &'a str,
    value: &'a str,
    short: bool,
}

fn first_n_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => s.get(..idx).unwrap_or(s),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::event::Severity;
    use uuid::Uuid;

    fn run_id() -> Uuid {
        Uuid::nil()
    }

    #[test]
    fn rejects_non_slack_webhook_url() {
        let err = SlackNotifier::from_webhook("https://evil.example.com/hook")
            .expect_err("should reject");
        assert!(matches!(err, NotifyError::InvalidConfig(_)));
    }

    #[test]
    fn accepts_well_formed_slack_webhook() {
        let notifier =
            SlackNotifier::from_webhook("https://hooks.slack.com/services/T00/B00/abc123");
        assert!(notifier.is_ok());
    }

    #[test]
    fn payload_has_severity_color() {
        let notifier =
            SlackNotifier::from_webhook("https://hooks.slack.com/services/T00/B00/abc123").unwrap();

        let event = NotificationEvent::migration_completed(run_id(), 15, 42);
        let payload = notifier.build_payload(&event);
        assert_eq!(
            payload.attachments[0].color,
            Severity::Success.slack_color()
        );
        assert_eq!(payload.attachments[0].color, "#22C55E");
    }

    #[test]
    fn payload_renders_fields_as_attachment_rows() {
        let notifier =
            SlackNotifier::from_webhook("https://hooks.slack.com/services/T00/B00/abc123").unwrap();

        let event = NotificationEvent::cost_alert(run_id(), 100.0, 150.0, 50.0);
        let payload = notifier.build_payload(&event);
        let attachment = &payload.attachments[0];
        let field_titles: Vec<&str> = attachment.fields.iter().map(|f| f.title).collect();
        assert!(field_titles.contains(&"target_usd"));
        assert!(field_titles.contains(&"actual_usd"));
        assert!(field_titles.contains(&"delta_pct"));
    }

    #[test]
    fn payload_serializes_to_valid_slack_json() {
        let notifier =
            SlackNotifier::from_webhook("https://hooks.slack.com/services/T00/B00/abc123").unwrap();
        let notifier = SlackNotifier {
            config: SlackConfig::new("https://hooks.slack.com/x")
                .with_username("Terrashift")
                .with_icon_emoji(":hammer_and_wrench:")
                .with_channel("#migrations"),
            ..notifier
        };

        let event = NotificationEvent::approval_required(run_id(), 3, "scan,plan,apply");
        let payload = notifier.build_payload(&event);
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["username"], "Terrashift");
        assert_eq!(json["icon_emoji"], ":hammer_and_wrench:");
        assert_eq!(json["channel"], "#migrations");
        assert!(json["attachments"][0]["color"]
            .as_str()
            .unwrap()
            .starts_with('#'));
        assert!(
            json["text"].as_str().unwrap().contains("Approval required"),
            "text should mention the title; got {}",
            json["text"]
        );
    }
}
