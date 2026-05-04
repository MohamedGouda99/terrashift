// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Knowledge layer errors.
//!
//! Constitution: Article IV (failures must be loud — every variant carries
//! enough context to localize the problem without re-running).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("schema not found: provider='{provider}' version='{version}'")]
    NotFound { provider: String, version: String },

    #[error("attempted to overwrite pinned schema: provider='{provider}' version='{version}' — Article VI forbids re-fetch of pinned versions")]
    AlreadyPinned { provider: String, version: String },

    #[error("storage error: {0}")]
    Storage(#[from] sqlx::Error),

    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("invalid version format: {0} (expected semver-like e.g. '5.30.0')")]
    InvalidVersion(String),
}
