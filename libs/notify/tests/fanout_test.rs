// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Integration tests for `MultiNotifier` + the in-memory `StubNotifier`.

use terrashift_notify::{
    LogNotifier, MultiNotifier, NotificationEvent, Notifier, Severity, StubNotifier,
};
use uuid::Uuid;

fn run_id() -> Uuid {
    Uuid::new_v4()
}

#[tokio::test]
async fn multi_fans_out_to_every_child_on_success() {
    let stub_a = StubNotifier::new();
    let stub_b = StubNotifier::new();

    let multi = MultiNotifier::new()
        .with(Box::new(stub_a.clone()))
        .with(Box::new(stub_b.clone()));

    let event = NotificationEvent::migration_completed(run_id(), 5, 30);
    multi.notify(&event).await.expect("ok");

    assert_eq!(stub_a.snapshot().len(), 1);
    assert_eq!(stub_b.snapshot().len(), 1);
    assert_eq!(stub_a.snapshot()[0].title, "Migration completed");
}

#[tokio::test]
async fn multi_keeps_going_when_one_child_fails() {
    let stub_failing = StubNotifier::new();
    stub_failing.arm_failure();
    let stub_ok = StubNotifier::new();

    let multi = MultiNotifier::new()
        .with(Box::new(stub_failing.clone()))
        .with(Box::new(stub_ok.clone()));

    // Returns Ok because at least one child succeeded.
    let event = NotificationEvent::migration_started(run_id(), "aws", "azure", 10);
    multi
        .notify(&event)
        .await
        .expect("at least one child must succeed");

    // The non-failing child still received the event.
    assert_eq!(stub_ok.snapshot().len(), 1);
    // The failing child captured nothing because the failure path
    // returns before the capture push.
    assert_eq!(stub_failing.snapshot().len(), 0);
}

#[tokio::test]
async fn multi_all_failed_returns_error() {
    let stub_a = StubNotifier::new();
    let stub_b = StubNotifier::new();
    stub_a.arm_failure();
    stub_b.arm_failure();

    let multi = MultiNotifier::new()
        .with(Box::new(stub_a))
        .with(Box::new(stub_b));

    let event = NotificationEvent::migration_failed(run_id(), "executor", "docker not running");
    let err = multi.notify(&event).await;
    assert!(err.is_err(), "all-failed should propagate an error");
}

#[tokio::test]
async fn empty_multi_is_noop_ok() {
    let multi = MultiNotifier::new();
    assert!(multi.is_empty());
    let event = NotificationEvent::cost_alert(run_id(), 100.0, 250.0, 150.0);
    multi
        .notify(&event)
        .await
        .expect("empty multi should be Ok");
}

#[tokio::test]
async fn log_notifier_does_not_panic_on_any_severity() {
    let log = LogNotifier::new();
    for sev in [
        Severity::Info,
        Severity::Action,
        Severity::Success,
        Severity::Warn,
        Severity::Error,
    ] {
        let event = NotificationEvent::new(run_id(), sev, "Test", "body");
        log.notify(&event).await.expect("log notifier never errors");
    }
}

#[tokio::test]
async fn stub_clear_resets_capture_buffer() {
    let stub = StubNotifier::new();
    let event = NotificationEvent::approval_required(run_id(), 2, "scan,plan");
    stub.notify(&event).await.expect("ok");
    assert_eq!(stub.snapshot().len(), 1);
    stub.clear();
    assert_eq!(stub.snapshot().len(), 0);
}

#[tokio::test]
async fn canonical_event_builders_include_expected_fields() {
    let id = run_id();

    let started = NotificationEvent::migration_started(id, "aws", "azure", 12);
    assert_eq!(started.severity, Severity::Info);
    assert!(started.fields.iter().any(|(k, _)| k == "source"));
    assert!(started.fields.iter().any(|(k, _)| k == "target"));

    let completed = NotificationEvent::migration_completed(id, 12, 60);
    assert_eq!(completed.severity, Severity::Success);

    let failed = NotificationEvent::migration_failed(id, "validator", "schema mismatch");
    assert_eq!(failed.severity, Severity::Error);

    let approval = NotificationEvent::approval_required(id, 3, "scan,plan,apply");
    assert_eq!(approval.severity, Severity::Action);

    let cost = NotificationEvent::cost_alert(id, 100.0, 150.0, 50.0);
    assert_eq!(cost.severity, Severity::Warn);

    let recovery = NotificationEvent::recovery_exhausted(id, 5, 2);
    assert_eq!(recovery.severity, Severity::Error);
}
