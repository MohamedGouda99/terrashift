// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Audit log errors.
//!
//! Constitution: Article IV (loud failures); Article V (audit is the
//! enforcement substrate — losing audit context is a security incident).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("storage error: {0}")]
    Storage(#[from] sqlx::Error),

    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("signing failed: {0}")]
    Signing(String),

    #[error("signature verification failed for entry {entry_id}: {reason}")]
    SignatureInvalid { entry_id: String, reason: String },

    #[error("hash chain broken at entry {entry_id}: {reason}")]
    ChainBroken { entry_id: String, reason: String },

    #[error("run not found: {run_id}")]
    RunNotFound { run_id: String },

    #[error("regex compile error: {0}")]
    RegexCompile(#[from] regex::Error),

    #[error("invalid signing key: {0}")]
    InvalidKey(String),
}
