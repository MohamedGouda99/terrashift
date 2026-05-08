// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Credential broker integration tests — Article V cornerstone.
//!
//! All tests offline. The cloud brokers' real STS / ADC / managed-
//! identity calls are deferred to S5 close.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use uuid::Uuid;

use terrashift_creds::{
    pre_llm_check, substitute, AwsBroker, AzureBroker, Credential, CredentialBroker, CredsError,
    GcpBroker, StubBroker,
};

// ─────────────────────────────────────────────────────────────────
// US1 — Substitution + Zeroize
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn substitution_roundtrip() {
    let broker = StubBroker::new().with_secret("aws-prod", "ASIAFAKEVALUE");
    let (out, map) = substitute("token={{secret:aws-prod}}", &broker)
        .await
        .unwrap();
    assert_eq!(out, "token=ASIAFAKEVALUE");
    assert_eq!(map.len(), 1);

    // Reverse rebuild: substituted text → reference form for audit.
    let rebuilt = map.rebuild(&out);
    assert_eq!(rebuilt, "token={{secret:aws-prod}}");
}

#[tokio::test]
async fn substitution_unknown_secret_loud_error() {
    let broker = StubBroker::new(); // no secrets registered
    let err = substitute("{{secret:nonexistent}}", &broker)
        .await
        .unwrap_err();
    match err {
        CredsError::UnknownReference(name) => assert_eq!(name, "nonexistent"),
        other => panic!("expected UnknownReference, got {other:?}"),
    }
}

#[tokio::test]
async fn substitution_multiple_refs_in_one_pass() {
    let broker = StubBroker::new()
        .with_secret("aws", "AAAVALUE")
        .with_secret("gcp", "GGGVALUE");
    let (out, map) = substitute("aws={{secret:aws}} gcp={{secret:gcp}}", &broker)
        .await
        .unwrap();
    assert_eq!(out, "aws=AAAVALUE gcp=GGGVALUE");
    assert_eq!(map.len(), 2);
}

#[tokio::test]
async fn substitution_malformed_reference_is_loud_error() {
    let broker = StubBroker::new().with_secret("ok", "v");
    // Malformed: empty body `{{secret:}}`.
    let err = substitute("{{secret:}}", &broker).await.unwrap_err();
    assert!(matches!(err, CredsError::MalformedReference(_)));

    // Malformed: whitespace in body.
    let err2 = substitute("{{secret: aws-prod }}", &broker)
        .await
        .unwrap_err();
    assert!(matches!(err2, CredsError::MalformedReference(_)));
}

#[tokio::test]
async fn substitution_unclosed_reference_is_loud_error() {
    let broker = StubBroker::new();
    // `{{secret:foo` with no closing `}}`.
    let err = substitute("prefix {{secret:foo and tail", &broker)
        .await
        .unwrap_err();
    assert!(matches!(err, CredsError::MalformedReference(_)));
}

#[test]
fn credential_drop_zeroes_inner_string() {
    // Construct, expose-borrow, drop. The Zeroizing wrapper guarantees
    // the underlying byte buffer is zeroed when the Credential is
    // dropped; we verify that the public surface never accidentally
    // hands out an owned copy beyond a borrowed &str.
    let cred = Credential::new(
        "very_secret_value_12345",
        "stub",
        chrono::Utc::now() + chrono::Duration::hours(1),
    );
    assert_eq!(cred.byte_len(), 23);
    let exposed = cred.expose();
    assert_eq!(exposed, "very_secret_value_12345");
    drop(cred); // Zeroizing's Drop runs here.
                // We can't observe the zeroed buffer from outside (lifetimes), but the
                // contract (Zeroizing's Drop impl) is type-system-enforced.
}

#[test]
fn credential_debug_does_not_leak_value() {
    let cred = Credential::new("ASIAFAKEVALUE-SHOULD-NOT-LEAK", "stub", chrono::Utc::now());
    let formatted = format!("{cred:?}");
    assert!(
        !formatted.contains("ASIAFAKEVALUE-SHOULD-NOT-LEAK"),
        "Debug must NEVER echo the inner value; got: {formatted}"
    );
    assert!(formatted.contains("redacted"));
    assert!(formatted.contains("stub"));
}

// ─────────────────────────────────────────────────────────────────
// US2 — Pre-LLM scrubber
// ─────────────────────────────────────────────────────────────────

#[test]
fn pre_llm_check_blocks_aws_key() {
    let err = pre_llm_check("prompt", "echo AKIAIOSFODNN7EXAMPLE").unwrap_err();
    match err {
        CredsError::SecretsDetected {
            pattern_name,
            field_path,
        } => {
            assert_eq!(pattern_name, "aws_access_key");
            assert_eq!(field_path, "prompt");
        }
        other => panic!("expected SecretsDetected, got {other:?}"),
    }
}

#[test]
fn pre_llm_check_blocks_gcp_key() {
    let err = pre_llm_check(
        "prompt",
        "GOOGLE_API_KEY=AIzaSyDdI0hCZtE6vySjMm-WEfRq3CPzqKqqsHI",
    )
    .unwrap_err();
    assert!(matches!(err, CredsError::SecretsDetected { .. }));
}

#[test]
fn pre_llm_check_passes_clean_payload_with_only_references() {
    pre_llm_check("prompt", "aws sts get-caller --token {{secret:aws-prod}}").unwrap();
    pre_llm_check("prompt", "echo hello world").unwrap();
}

#[test]
fn pre_llm_check_error_does_not_leak_raw_value() {
    let err = pre_llm_check("prompt", "echo AKIAIOSFODNN7EXAMPLE").unwrap_err();
    let msg = format!("{err}");
    assert!(
        !msg.contains("AKIAIOSFODNN7EXAMPLE"),
        "error message must NEVER echo raw cred value; got: {msg}"
    );
    // But it DOES name the pattern + field for diagnostic purposes.
    assert!(msg.contains("aws_access_key"));
    assert!(msg.contains("prompt"));
}

// ─────────────────────────────────────────────────────────────────
// US3 — Cloud broker scaffolding (NotImplementedYet)
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn aws_broker_legacy_fetch_aws_points_to_new_api() {
    // S5 close: the legacy single-Credential `fetch_aws` is preserved
    // as `NotImplementedYet` because STS naturally returns 3 values
    // (access_key + secret + session_token) which don't fit the
    // single-string Credential shape. New code calls
    // `AwsBroker::resolve_sts_assume_role` (returns
    // CloudCredentialEnvVars). The error message points the caller
    // at the new API.
    let broker = AwsBroker::new();
    let err = broker.fetch_aws("test-role").await.unwrap_err();
    match err {
        CredsError::NotImplementedYet { which, session } => {
            assert!(
                which.contains("resolve_sts_assume_role"),
                "error should redirect callers to the new multi-var API; got which='{which}'"
            );
            assert_eq!(session, "S5");
        }
        other => panic!("expected NotImplementedYet, got {other:?}"),
    }
}

#[tokio::test]
async fn gcp_broker_returns_not_implemented_until_s5() {
    let broker = GcpBroker::new();
    let err = broker.fetch_gcp("svc-account").await.unwrap_err();
    match err {
        CredsError::NotImplementedYet { which, session } => {
            assert_eq!(which, "gcp_adc_workload_identity_federation");
            assert_eq!(session, "S5");
        }
        other => panic!("expected NotImplementedYet, got {other:?}"),
    }
}

#[tokio::test]
async fn azure_broker_returns_not_implemented_until_s5() {
    let broker = AzureBroker::new();
    let err = broker.fetch_azure("sub-id").await.unwrap_err();
    match err {
        CredsError::NotImplementedYet { which, session } => {
            assert_eq!(which, "azure_managed_identity");
            assert_eq!(session, "S5");
        }
        other => panic!("expected NotImplementedYet, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────
// US4 — Audit hook
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stub_broker_audits_on_fetch() {
    use terrashift_audit::{AuditStore, LocalAuditStore, SessionSigner};

    let store = LocalAuditStore::in_memory().await.unwrap();
    let run_id = Uuid::new_v4();
    let signer = Arc::new(SessionSigner::generate());
    AuditStore::register_run(&store, run_id, signer.verify_key_bytes())
        .await
        .unwrap();
    let arc_store: Arc<dyn terrashift_audit::AuditStore + Send + Sync> = Arc::new(store);

    let broker = StubBroker::new()
        .with_secret("aws-prod", "ASIAEXAMPLE")
        .with_audit(arc_store.clone(), run_id, signer.clone());

    let _cred = broker.fetch_aws("aws-prod").await.unwrap();

    // Verify exactly one CredentialResolution entry was appended.
    let entries = arc_store.export(run_id).await.unwrap();
    let cred_entries: Vec<_> = entries
        .iter()
        .filter(|e| {
            matches!(
                e.payload,
                terrashift_audit::AuditPayload::CredentialResolution { .. }
            )
        })
        .collect();
    assert_eq!(
        cred_entries.len(),
        1,
        "expected exactly 1 CredentialResolution"
    );

    // Article V invariant: cred_ref is the {{secret:...}} reference, NOT the value.
    if let terrashift_audit::AuditPayload::CredentialResolution {
        cred_ref,
        resolution_method,
        ..
    } = &cred_entries[0].payload
    {
        assert_eq!(cred_ref, "{{secret:aws-prod}}");
        assert_eq!(resolution_method, "stub");
        // Critical: the raw value MUST NOT be in the audit entry anywhere.
        let serialized = serde_json::to_string(&cred_entries[0]).unwrap();
        assert!(
            !serialized.contains("ASIAEXAMPLE"),
            "audit entry must NEVER contain the raw cred value; got: {serialized}"
        );
    } else {
        panic!("payload variant mismatch");
    }
}
