// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `VerifierError` — infrastructure failure modes.
//!
//! Pattern: mirrors `validator::errors::ValidatorError`.
//! Constitution: Article IV (every error names what failed and why).

use thiserror::Error;

/// Anything that prevents Verifier from producing a `VerifierReport`.
///
/// Drift findings (`MissingResource`, `ExtraResource`, `TypeMismatch`)
/// land in the *report*, not here. This enum only carries failures that
/// stop the comparison from happening at all.
#[derive(Debug, Error)]
pub enum VerifierError {
    /// `terraform show -json` output didn't parse as JSON, or didn't
    /// match the expected schema. The serde error is preserved verbatim
    /// for compliance audit.
    #[error("failed to parse terraform-show JSON: {0}")]
    ParseFailed(#[from] serde_json::Error),

    /// State file's `format_version` isn't `"1.x"`. Stage 1 ships against
    /// Terraform 1.10+ only (matches the DockerRunner pin in PR #12).
    #[error("unsupported terraform-show format_version: '{0}' (expected '1.x'; Stage 1 ships against Terraform 1.10+)")]
    UnsupportedFormatVersion(String),

    /// Plan and state belong to different providers. Cheapest possible
    /// failure mode — operator pointed Verifier at the wrong state file.
    #[error("provider mismatch: plan targets '{expected}', state contains '{actual}' (likely the operator pointed Verifier at the wrong state file)")]
    ProviderMismatch { expected: String, actual: String },
}
