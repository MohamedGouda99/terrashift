// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Credential broker failure modes.
//!
//! Pattern: matches the per-crate `errors.rs` shape across the workspace
//! (Scanner, Generator, Validator, Mapper, AI, Eval). Every failure mode
//! named, no anonymous strings.
//! Constitution: Article IV (loud), Article V (error messages NEVER echo
//! raw credential values; field paths + pattern names only).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CredsError {
    /// `substitute()` saw `{{secret:NAME}}` but the broker has no
    /// credential for `NAME`. Article IV: loud, names the missing
    /// reference.
    #[error("unknown credential reference: '{0}' (no broker entry; check ~/.terrashift/config.toml or the broker registration)")]
    UnknownReference(String),

    /// `{{secret:}}`, `{{secret: foo }}`, `{{secret:foo{{secret:bar}}}}`,
    /// or anything else that doesn't fit the strict
    /// `{{secret:[A-Za-z0-9_-]+}}` shape. Article IV.
    #[error("malformed credential reference: '{0}' (expected '{{{{secret:NAME}}}}' with NAME matching [A-Za-z0-9_-]+)")]
    MalformedReference(String),

    /// `pre_llm_check` caught a raw secret pattern. NEVER echoes the
    /// raw value — only the pattern name + field path. Article XIII
    /// rule 5 (proactive layer; the audit-writer panic is the
    /// runtime layer).
    #[error("secret detected by pre-LLM scrubber: pattern='{pattern_name}' at field='{field_path}' (raw value redacted)")]
    SecretsDetected {
        pattern_name: String,
        field_path: String,
    },

    /// Stage 1 placeholder for cloud broker methods not yet wired.
    /// Names the unblocking session so callers know exactly what's
    /// missing. Article IV.
    #[error("not implemented yet: '{which}' wires real cloud SDK calls (unblocks in {session}; needs cloud test creds + Docker)")]
    NotImplementedYet {
        which: &'static str,
        session: &'static str,
    },

    /// Wrapped audit-store error (e.g., disk full when appending
    /// `CredentialResolution`). Boxed for clippy `result_large_err`
    /// because `terrashift_audit::AuditError` carries underlying
    /// `sqlx::Error` which is large.
    #[error("audit append failed: {0}")]
    Audit(Box<terrashift_audit::AuditError>),

    /// Federated token exchange (S12 — `FederatedTokenProvider`) failed
    /// at the operator's mutex / internal state boundary. Generic
    /// resolution-error catch-all that names the reference and the
    /// cause without echoing any token contents (Article V).
    #[error("federated token resolution failed: name='{name}' cause='{cause}'")]
    ResolutionError { name: String, cause: String },
}
