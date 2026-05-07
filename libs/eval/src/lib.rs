// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Terrashift Eval framework — golden-file harness for the deterministic
//! pipeline.
//!
//! Pattern: the architecture reference §32 (CI matrix; eval suite is one of the
//! gated checks). Source: the reference codebase (see ATTRIBUTIONS.md)
//! (single-step feature-gated invocations — adopted in S7 expansion;
//! Stage 1 runs inline via `cargo test --workspace`).
//!
//! Constitution: Article III (evals are source of truth for AI safety),
//! Article XII rule 4 (regression-gate plumbing now; full activation S7),
//! Article XIII rule 2 (cache-stability surfaces via determinism gate).
//!
//! ## Public API
//!
//! ```no_run
//! use terrashift_eval::EvalRunner;
//! use std::path::Path;
//!
//! let runner = EvalRunner::new();
//! let report = runner.run_suite(Path::new("terrashift-evals"))?;
//! assert!(report.all_passed(), "{} of {} goldens failed", report.failed, report.total);
//! # Ok::<_, terrashift_eval::EvalError>(())
//! ```
//!
//! ## Modules
//! - `errors`   — `EvalError` enum
//! - `golden`   — fixture loading + suite discovery
//! - `scorer`   — directory-level byte comparison + unified diff
//! - `runner`   — `EvalRunner`, `EvalResult`, `SuiteReport`
//! - `baseline` — Article XII rule 4 token-cost regression gate (R6)

pub mod baseline;
pub mod errors;
pub mod golden;
pub mod runner;
pub mod scorer;

pub use baseline::{
    compare_against_baseline, default_baseline_path, Baseline, BaselineMode, FixtureBaseline,
    FixtureDelta, RegressionReport, RegressionVerdict, REGRESSION_THRESHOLD_PCT,
};
pub use errors::EvalError;
pub use golden::{discover_suite, load_golden, GoldenManifest, GoldenMigration, RequiredSchema};
pub use runner::{EvalResult, EvalRunner, SuiteReport};
pub use scorer::{compare_directories, ComparisonResult};
