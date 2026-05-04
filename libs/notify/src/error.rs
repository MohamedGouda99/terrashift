// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Notification error taxonomy.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotifyError {
    /// HTTP request to the notification endpoint failed (timeout,
    /// DNS, TLS error, etc.). Boxed to keep the error variant size
    /// small per clippy `result_large_err`.
    #[error("network error: {0}")]
    Network(Box<reqwest::Error>),

    /// Endpoint returned a non-2xx status. Carries the status code +
    /// the first 200 chars of the body so operators can debug Slack
    /// 'invalid_payload' / 'channel_not_found' errors without leaking
    /// the full response.
    #[error("notify endpoint returned {status}: {body_excerpt}")]
    EndpointError { status: u16, body_excerpt: String },

    /// JSON serialization failure (notification payload couldn't be
    /// serialized — extremely unlikely for our shapes).
    #[error("json serialization failed: {0}")]
    Serialization(String),

    /// `SlackNotifier::from_webhook` rejected the URL — must start
    /// with `https://hooks.slack.com/`.
    #[error("invalid webhook URL: {0}")]
    InvalidConfig(String),
}

impl From<reqwest::Error> for NotifyError {
    fn from(err: reqwest::Error) -> Self {
        Self::Network(Box::new(err))
    }
}

impl From<serde_json::Error> for NotifyError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}
