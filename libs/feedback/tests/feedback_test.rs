// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use terrashift_feedback::{
    optin::{current_state, record_choice, should_prompt_for_optin, OptIn},
    telemetry::{record_event, Event},
    FeedbackCategory, FeedbackReport,
};

#[test]
fn url_encodes_message_and_includes_category() {
    let r = FeedbackReport {
        category: FeedbackCategory::Bug,
        message: "scan crashed when path has spaces & symbols".to_string(),
        run_id: None,
        version: "0.1.2".to_string(),
    };
    let url = r.to_github_issue_url("https://github.com/MohamedGouda99/terrashift");
    assert!(url.starts_with("https://github.com/MohamedGouda99/terrashift/issues/new?"));
    assert!(url.contains("template=feedback.md"));
    assert!(url.contains("labels=user-feedback"));
    assert!(url.contains("scan%20crashed%20when%20path%20has%20spaces"));
    assert!(url.contains("%26"));
}

#[test]
fn opt_in_default_is_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("telemetry.json");
    assert_eq!(current_state(Some(&path)).unwrap(), OptIn::Disabled);
    assert!(should_prompt_for_optin(Some(&path)).unwrap());
}

#[test]
fn opt_in_persists_across_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("telemetry.json");
    record_choice(OptIn::Enabled, Some(&path)).unwrap();
    assert_eq!(current_state(Some(&path)).unwrap(), OptIn::Enabled);
    assert!(!should_prompt_for_optin(Some(&path)).unwrap());
}

#[test]
fn opt_in_record_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("telemetry.json");
    record_choice(OptIn::Enabled, Some(&path)).unwrap();
    record_choice(OptIn::Disabled, Some(&path)).unwrap();
    assert_eq!(current_state(Some(&path)).unwrap(), OptIn::Disabled);
}

#[test]
fn telemetry_drops_event_when_disabled() {
    let ev = Event {
        name: "scan_completed".to_string(),
        properties: Default::default(),
    };
    record_event(OptIn::Disabled, ev).unwrap();
}
