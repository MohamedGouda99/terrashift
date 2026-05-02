//! Terrashift Eval framework — golden-file harness for the deterministic
//! pipeline.
//!
//! Pattern: stakpak_arch.md §32 (CI matrix; eval suite is one of the
//! gated checks). Source: refs/stakpak/.github/workflows/ci.yml:38-43
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
//! - `errors`  — `EvalError` enum
//! - `golden`  — fixture loading + suite discovery
//! - `scorer`  — directory-level byte comparison + unified diff
//! - `runner`  — `EvalRunner`, `EvalResult`, `SuiteReport`

pub mod errors;
pub mod golden;
pub mod runner;
pub mod scorer;

pub use errors::EvalError;
pub use golden::{discover_suite, load_golden, GoldenManifest, GoldenMigration};
pub use runner::{EvalResult, EvalRunner, SuiteReport};
pub use scorer::{compare_directories, ComparisonResult};
