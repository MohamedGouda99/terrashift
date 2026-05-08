// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Eval framework failure modes.
//!
//! Pattern: mirrors `libs/engine/src/scanner/errors.rs` shape — one
//! `thiserror` enum per concern. Constitution: Article IV (every failure
//! mode named).

use std::path::PathBuf;
use thiserror::Error;

/// Top-level eval framework failure modes.
#[derive(Debug, Error)]
pub enum EvalError {
    /// Suite root doesn't exist or isn't a directory.
    #[error("suite root '{0}' is not a directory")]
    InvalidSuiteRoot(PathBuf),

    /// Per-fixture root doesn't exist or lacks a `manifest.toml`.
    #[error("golden fixture at '{path}' is missing required file: {missing}")]
    MissingFixtureFile {
        path: PathBuf,
        missing: &'static str,
    },

    /// `manifest.toml` exists but doesn't parse. The `toml::de::Error`
    /// is boxed because it's ~120 bytes — clippy `result_large_err`
    /// fires on `Result<T, EvalError>` otherwise.
    #[error("manifest at '{path}': {source}")]
    ManifestParse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    /// `mapping_plan.json` exists but doesn't parse. Boxed for the same
    /// `result_large_err` reason as `ManifestParse`.
    #[error("mapping_plan at '{path}': {source}")]
    MappingPlanParse {
        path: PathBuf,
        #[source]
        source: Box<serde_json::Error>,
    },

    /// Filesystem I/O.
    #[error("filesystem I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Wraps a Generator failure during a fixture run. The fixture's
    /// `name` is included so the SuiteReport can attribute it.
    #[error("generator failed for fixture '{name}': {source}")]
    Generator {
        name: String,
        #[source]
        source: terrashift_engine::generator::GeneratorError,
    },

    /// Per-fixture comparison failed structurally (not the same as
    /// `EvalResult.passed = false` — that's expected drift; this is e.g.
    /// "expected/ has files actual doesn't, or vice versa, structurally").
    /// Used for diagnostics, not test failure.
    #[error("comparison error at fixture '{name}': {detail}")]
    Comparison { name: String, detail: String },

    /// A `required_schemas` entry has no matching file in the eval
    /// fixture's `.schema-cache/`. Reusing `MissingFixtureFile` is not an
    /// option because that variant's `missing` field is `&'static str`;
    /// the schema id is constructed at runtime as `<provider>@<version>`.
    /// RFC schema-source-migration §OQ-5.
    #[error(
        "golden fixture at '{fixture}' requires schema '{schema_id}' \
         which is not present in the eval schema cache — run: \
         cargo xtask capture-eval-schemas"
    )]
    MissingSchema { fixture: PathBuf, schema_id: String },
}
