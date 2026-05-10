// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Typed errors for the cost-optimizer crate.
//!
//! Article IV: every failure mode is named, never bagged into a string.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CostOptimizerError {
    #[error("infracost api key not configured: env var '{0}' is not set")]
    ApiKeyMissing(String),

    #[error(
        "infracost rate limit hit after {attempts} attempts (retry-after: {retry_after_secs}s)"
    )]
    RateLimitExhausted {
        attempts: u32,
        retry_after_secs: u64,
    },

    #[error("infracost network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("infracost api returned an error: {0}")]
    Api(String),

    #[error("infracost response parse failure: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("cost cache error: {0}")]
    Cache(#[from] crate::cache::CostCacheError),

    #[error("home directory not found (USERPROFILE / HOME unset)")]
    HomeMissing,

    #[error("io error at {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
}
