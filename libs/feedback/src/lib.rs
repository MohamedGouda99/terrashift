// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! User-relationship state for Terrashift.
//!
//! Three modules:
//! - `lib`        — `FeedbackReport` + `to_github_issue_url`
//! - `optin`      — opt-in state persisted to `~/.terrashift/telemetry.json`
//! - `telemetry`  — no-op stub gated behind the `telemetry-net` feature

pub mod optin;
pub mod telemetry;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeedbackCategory {
    Bug,
    Idea,
    Other,
}

impl FeedbackCategory {
    fn as_label(&self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Idea => "idea",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackReport {
    pub category: FeedbackCategory,
    pub message: String,
    pub run_id: Option<uuid::Uuid>,
    pub version: String,
}

impl FeedbackReport {
    /// Build a pre-filled GitHub issue URL the operator can open.
    pub fn to_github_issue_url(&self, repo: &str) -> String {
        let title = format!("[{}] {}", self.category.as_label(), truncate(&self.message, 60));
        let mut body = String::new();
        body.push_str("**Category:** ");
        body.push_str(self.category.as_label());
        body.push_str("\n\n**Message:**\n\n");
        body.push_str(&self.message);
        body.push_str("\n\n**Version:** ");
        body.push_str(&self.version);
        if let Some(id) = self.run_id {
            body.push_str("\n**Run ID:** ");
            body.push_str(&id.to_string());
        }

        let mut q: BTreeMap<&str, &str> = BTreeMap::new();
        q.insert("template", "feedback.md");
        q.insert("title", title.as_str());
        q.insert("body", body.as_str());
        q.insert("labels", "user-feedback");

        format!(
            "{}/issues/new?{}",
            repo.trim_end_matches('/'),
            url_encode(&q)
        )
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

fn url_encode(pairs: &BTreeMap<&str, &str>) -> String {
    let mut out = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&percent_encode(k));
        out.push('=');
        out.push_str(&percent_encode(v));
    }
    out
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}
