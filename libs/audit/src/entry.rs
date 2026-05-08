// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Audit entry types — the on-disk schema that compliance reviewers will read.
//!
//! Pattern: terrashift_plan.md §6.X (full AuditPayload variants).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (envelope shape inspiration).
//! Constitution: Article V (audit invariant — every LLM-touching op has provider+model_id+endpoint),
//! Article IX (append-only, never auto-deleted).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Top-level entry. Each row in the audit log is exactly one of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub run_id: Uuid,
    pub actor: Actor,
    pub operation: String, // e.g. "tool.execute", "phase.transition", "llm.call"
    pub outcome: Outcome,
    pub payload: AuditPayload,

    // Hash chain — populated by AuditStore.append() at write time
    pub prev_hash: [u8; 32],
    pub content_hash: [u8; 32],
    pub signature: Vec<u8>, // 64 bytes for Ed25519
}

/// Who initiated the operation. Populated at the call site (CLI or sub-agent).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    User { id: String },
    System,
    Agent { name: String },
}

/// Did the operation succeed or fail? Compliance reviewers want this
/// explicit — silent failure is the worst outcome (Article IV).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Err { kind: String, message: String },
}

/// Operation-specific data. Each variant is shaped by what auditors need
/// to answer their typical questions.
///
/// Per Article V: `LlmCall` MUST carry `provider`, `model_id`, `provider_endpoint`
/// so reviewers can answer "which model produced this artifact?" deterministically.
///
/// Per Article XIII rule 5: any string field here is REJECTED at write time
/// if the scrubber detects a raw secret pattern. The scrubber is the LAST
/// line of defence — see scrubber.rs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditPayload {
    /// One LLM inference call. cost_usd_micros is computed by libs/ai at
    /// call time from the active model's price card (S4).
    LlmCall {
        provider: String,
        model_id: String,
        provider_endpoint: String,
        tier: String, // "eco" | "smart"
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
        cache_write_tokens: u32,
        cost_usd_micros: u64,
    },

    /// One tool execution.
    ToolExecution {
        tool_name: String,
        cred_ref: Option<String>, // ALWAYS the {{secret:name}} reference, NEVER resolved
        target: String,
        duration_ms: u32,
    },

    /// Migration moved from one phase to another (Discover → Plan → Generate → ...).
    PhaseTransition {
        from: String,
        to: String,
        checkpoint_id: Option<Uuid>,
    },

    /// Cred broker resolved a secret reference. cred_ref stays the placeholder;
    /// resolution_method is e.g. "sts_assume_role" / "gcp_adc" / "azure_managed_identity".
    CredentialResolution {
        cred_ref: String,
        resolution_method: String,
        expires_at: DateTime<Utc>,
    },

    /// File system mutation (Generator emits HCL, Executor writes state).
    FileOperation {
        path: PathBuf,
        kind: FileOpKind,
        backup_path: Option<PathBuf>,
    },

    /// Provider schema captured via the `terraform providers schema -json`
    /// subprocess. One entry per (provider, version) per capture invocation.
    ///
    /// Per Article V: every schema in the system is traceable to who captured
    /// it, when, against which Terraform version, and with which constraint
    /// expansion. Reviewers can answer "where did the AWS 5.30.0 schema in
    /// this run come from?" deterministically.
    ///
    /// Per Article VI: `version_constraint` records what the user asked for
    /// (e.g. `"~> 5.30"`); `resolved_version` records what was actually
    /// captured (e.g. `"5.30.4"`). The pair is the audit trail for any
    /// "did this migration use the same schema as the prior run?" question.
    SchemaCapture {
        provider: String,           // e.g. "aws"
        source: String,             // e.g. "hashicorp/aws"
        version_constraint: String, // e.g. "~> 5.30"
        resolved_version: String,   // e.g. "5.30.4"
        terraform_version: String,  // e.g. "1.7.5"
        captured_via: CaptureVia,
        sha256: String, // 64-char lowercase hex of the captured schema.json
        duration_ms: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileOpKind {
    Create,
    Modify,
    Delete,
}

/// How a `SchemaCapture` was triggered. Recorded in the audit log so
/// reviewers can distinguish "this schema came with the binary" from
/// "the operator ran `terrashift schema update`" from "the auto-update
/// flow swapped this in" — three different trust contexts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "via", rename_all = "snake_case")]
pub enum CaptureVia {
    /// Captured at `cargo build --release` via the xtask; bundled with the
    /// release binary and extracted into `~/.terrashift/schemas/` on first run.
    Bundled,
    /// Captured by an explicit `terrashift schema update` invocation.
    UserCommand,
    /// Captured by the background auto-update flow during `terrashift migrate`
    /// startup; user authorization required before swap (Article VI).
    AutoUpdate,
}

impl AuditEntry {
    /// Construct a fresh entry. The hash chain fields are zeroed; AuditStore
    /// fills them in at append time.
    pub fn new(
        run_id: Uuid,
        actor: Actor,
        operation: impl Into<String>,
        outcome: Outcome,
        payload: AuditPayload,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            run_id,
            actor,
            operation: operation.into(),
            outcome,
            payload,
            prev_hash: [0u8; 32],
            content_hash: [0u8; 32],
            signature: Vec::new(),
        }
    }
}
