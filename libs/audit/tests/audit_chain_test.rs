// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Integration tests for the audit log: round-trip, tamper detection, redaction-panic.

use std::path::PathBuf;
use terrashift_audit::{
    verify_chain, Actor, AuditEntry, AuditPayload, AuditStore, CaptureVia, FileOpKind,
    LocalAuditStore, Outcome, SessionSigner,
};
use uuid::Uuid;

async fn fresh_store() -> (LocalAuditStore, SessionSigner, Uuid) {
    let store = LocalAuditStore::in_memory().await.expect("store");
    let signer = SessionSigner::generate();
    let run_id = Uuid::new_v4();
    store
        .register_run(run_id, signer.verify_key_bytes())
        .await
        .expect("register run");
    (store, signer, run_id)
}

fn tool_exec_payload(name: &str) -> AuditPayload {
    AuditPayload::ToolExecution {
        tool_name: name.to_string(),
        cred_ref: Some("{{secret:aws-prod}}".to_string()),
        target: "aws_vpc.main".to_string(),
        duration_ms: 42,
    }
}

#[tokio::test]
async fn append_and_export_round_trip() {
    let (store, signer, run_id) = fresh_store().await;

    let mut e1 = AuditEntry::new(
        run_id,
        Actor::System,
        "tool.execute",
        Outcome::Ok,
        tool_exec_payload("scan"),
    );
    store.append(&signer, &mut e1).await.expect("append e1");

    let mut e2 = AuditEntry::new(
        run_id,
        Actor::System,
        "tool.execute",
        Outcome::Ok,
        tool_exec_payload("validate"),
    );
    store.append(&signer, &mut e2).await.expect("append e2");

    let entries = store.export(run_id).await.expect("export");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, e1.id);
    assert_eq!(entries[1].id, e2.id);
    // Chain link
    assert_eq!(entries[1].prev_hash, entries[0].content_hash);
    assert_eq!(entries[0].prev_hash, [0u8; 32]);
}

#[tokio::test]
async fn verify_chain_clean_run_passes() {
    let (store, signer, run_id) = fresh_store().await;

    for op in ["scan", "map", "validate", "generate"] {
        let mut e = AuditEntry::new(
            run_id,
            Actor::Agent {
                name: "agent-core".to_string(),
            },
            "tool.execute",
            Outcome::Ok,
            tool_exec_payload(op),
        );
        store.append(&signer, &mut e).await.expect("append");
    }

    let entries = store.export(run_id).await.expect("export");
    let key = store.get_verify_key(run_id).await.expect("verify key");
    verify_chain(&entries, &key).expect("clean chain should verify");
}

#[tokio::test]
async fn tampering_with_content_breaks_chain() {
    let (store, signer, run_id) = fresh_store().await;

    let mut e1 = AuditEntry::new(
        run_id,
        Actor::System,
        "tool.execute",
        Outcome::Ok,
        tool_exec_payload("scan"),
    );
    store.append(&signer, &mut e1).await.expect("append");

    let mut entries = store.export(run_id).await.expect("export");
    // Tamper: modify the operation field but leave the stored content_hash unchanged
    entries[0].operation = "tool.malicious".to_string();

    let key = store.get_verify_key(run_id).await.expect("key");
    let err = verify_chain(&entries, &key).expect_err("tampered entry should fail");
    assert!(matches!(
        err,
        terrashift_audit::AuditError::ChainBroken { .. }
    ));
}

#[tokio::test]
async fn tampering_with_signature_is_detected_separately() {
    let (store, signer, run_id) = fresh_store().await;

    let mut e1 = AuditEntry::new(
        run_id,
        Actor::System,
        "tool.execute",
        Outcome::Ok,
        tool_exec_payload("scan"),
    );
    store.append(&signer, &mut e1).await.expect("append");

    let mut entries = store.export(run_id).await.expect("export");
    // Corrupt the signature (flip a byte) — content_hash still recomputes correctly,
    // so this should fail at the SignatureInvalid check, not ChainBroken.
    entries[0].signature[0] ^= 0xff;

    let key = store.get_verify_key(run_id).await.expect("key");
    let err = verify_chain(&entries, &key).expect_err("bad signature should fail");
    assert!(matches!(
        err,
        terrashift_audit::AuditError::SignatureInvalid { .. }
    ));
}

#[tokio::test]
async fn multi_run_isolation() {
    let store = LocalAuditStore::in_memory().await.expect("store");

    let signer_a = SessionSigner::generate();
    let signer_b = SessionSigner::generate();
    let run_a = Uuid::new_v4();
    let run_b = Uuid::new_v4();

    store
        .register_run(run_a, signer_a.verify_key_bytes())
        .await
        .expect("reg A");
    store
        .register_run(run_b, signer_b.verify_key_bytes())
        .await
        .expect("reg B");

    let mut a1 = AuditEntry::new(
        run_a,
        Actor::System,
        "tool.execute",
        Outcome::Ok,
        tool_exec_payload("scan"),
    );
    store.append(&signer_a, &mut a1).await.expect("a1");

    let mut b1 = AuditEntry::new(
        run_b,
        Actor::System,
        "tool.execute",
        Outcome::Ok,
        tool_exec_payload("validate"),
    );
    store.append(&signer_b, &mut b1).await.expect("b1");

    // Each run's chain verifies independently
    let entries_a = store.export(run_a).await.expect("export A");
    let entries_b = store.export(run_b).await.expect("export B");
    let key_a = store.get_verify_key(run_a).await.expect("key A");
    let key_b = store.get_verify_key(run_b).await.expect("key B");

    verify_chain(&entries_a, &key_a).expect("A verifies");
    verify_chain(&entries_b, &key_b).expect("B verifies");
}

#[tokio::test]
async fn file_op_payload_records_path() {
    let (store, signer, run_id) = fresh_store().await;

    let payload = AuditPayload::FileOperation {
        path: PathBuf::from("./generated/main.tf"),
        kind: FileOpKind::Create,
        backup_path: None,
    };
    let mut e = AuditEntry::new(run_id, Actor::System, "file.write", Outcome::Ok, payload);
    store.append(&signer, &mut e).await.expect("append");

    let entries = store.export(run_id).await.expect("export");
    match &entries[0].payload {
        AuditPayload::FileOperation { path, kind, .. } => {
            assert_eq!(path, &PathBuf::from("./generated/main.tf"));
            assert_eq!(kind, &FileOpKind::Create);
        }
        other => panic!("expected FileOperation, got {other:?}"),
    }
}

#[tokio::test]
async fn llm_call_payload_records_provider_model_endpoint() {
    let (store, signer, run_id) = fresh_store().await;

    let payload = AuditPayload::LlmCall {
        provider: "groq".to_string(),
        model_id: "llama-3.3-70b-versatile".to_string(),
        provider_endpoint: "https://api.groq.com/openai/v1/chat/completions".to_string(),
        tier: "eco".to_string(),
        input_tokens: 1234,
        output_tokens: 567,
        cache_read_tokens: 200,
        cache_write_tokens: 0,
        cost_usd_micros: 12_500,
    };
    let mut e = AuditEntry::new(run_id, Actor::System, "llm.call", Outcome::Ok, payload);
    store.append(&signer, &mut e).await.expect("append");

    let entries = store.export(run_id).await.expect("export");
    match &entries[0].payload {
        AuditPayload::LlmCall {
            provider,
            model_id,
            provider_endpoint,
            ..
        } => {
            // Article V invariant: every LlmCall has provider + model_id + endpoint
            assert_eq!(provider, "groq");
            assert!(model_id.contains("llama"));
            assert!(provider_endpoint.starts_with("https://"));
        }
        other => panic!("expected LlmCall, got {other:?}"),
    }
}

#[tokio::test]
async fn schema_capture_payload_records_all_metadata() {
    let (store, signer, run_id) = fresh_store().await;

    let payload = AuditPayload::SchemaCapture {
        provider: "aws".to_string(),
        source: "hashicorp/aws".to_string(),
        version_constraint: "~> 5.30".to_string(),
        resolved_version: "5.30.4".to_string(),
        terraform_version: "1.7.5".to_string(),
        captured_via: CaptureVia::UserCommand,
        sha256: "a1f7e21d8c3b0429f5e8d6c4b8a9f2e1d3c5b7a9e0d2f4c6b8a1d3e5f7c9b1d3".to_string(),
        duration_ms: 14_287,
    };
    let mut e = AuditEntry::new(
        run_id,
        Actor::User {
            id: "operator@example.com".to_string(),
        },
        "schema.capture",
        Outcome::Ok,
        payload,
    );
    store.append(&signer, &mut e).await.expect("append");

    let entries = store.export(run_id).await.expect("export");
    match &entries[0].payload {
        AuditPayload::SchemaCapture {
            provider,
            source,
            version_constraint,
            resolved_version,
            terraform_version,
            captured_via,
            sha256,
            duration_ms,
        } => {
            // Article V invariant: every schema is traceable end-to-end
            assert_eq!(provider, "aws");
            assert_eq!(source, "hashicorp/aws");
            assert_eq!(version_constraint, "~> 5.30");
            assert_eq!(resolved_version, "5.30.4");
            assert_eq!(terraform_version, "1.7.5");
            assert_eq!(captured_via, &CaptureVia::UserCommand);
            assert_eq!(sha256.len(), 64);
            assert!(*duration_ms > 0);
        }
        other => panic!("expected SchemaCapture, got {other:?}"),
    }
}

#[tokio::test]
async fn tampering_with_resolved_version_breaks_chain() {
    let (store, signer, run_id) = fresh_store().await;

    let payload = AuditPayload::SchemaCapture {
        provider: "aws".to_string(),
        source: "hashicorp/aws".to_string(),
        version_constraint: "~> 5.30".to_string(),
        resolved_version: "5.30.4".to_string(),
        terraform_version: "1.7.5".to_string(),
        captured_via: CaptureVia::Bundled,
        sha256: "a1f7e21d8c3b0429f5e8d6c4b8a9f2e1d3c5b7a9e0d2f4c6b8a1d3e5f7c9b1d3".to_string(),
        duration_ms: 14_287,
    };
    let mut e = AuditEntry::new(
        run_id,
        Actor::System,
        "schema.capture",
        Outcome::Ok,
        payload,
    );
    store.append(&signer, &mut e).await.expect("append");

    let mut entries = store.export(run_id).await.expect("export");
    // Tamper: a malicious actor swaps the resolved_version to make a
    // different schema look like the audited one. Chain must catch this.
    if let AuditPayload::SchemaCapture {
        resolved_version, ..
    } = &mut entries[0].payload
    {
        *resolved_version = "5.31.0".to_string();
    } else {
        panic!("expected SchemaCapture variant");
    }

    let key = store.get_verify_key(run_id).await.expect("key");
    let err = verify_chain(&entries, &key).expect_err("tampered schema must fail");
    assert!(matches!(
        err,
        terrashift_audit::AuditError::ChainBroken { .. }
    ));
}

#[tokio::test]
#[should_panic(expected = "secret pattern")]
async fn raw_aws_key_in_payload_panics() {
    let (store, signer, run_id) = fresh_store().await;

    // RAW key embedded in the target field — would be a leak if we let this through.
    let payload = AuditPayload::ToolExecution {
        tool_name: "scan".to_string(),
        cred_ref: None,
        target: "leaked: AKIAIOSFODNN7EXAMPLE".to_string(),
        duration_ms: 1,
    };
    let mut e = AuditEntry::new(run_id, Actor::System, "tool.execute", Outcome::Ok, payload);
    // SHOULD PANIC per Article XIII rule 5 — "the writer is the LAST line of defence"
    let _ = store.append(&signer, &mut e).await;
}
