//! Eval runner — drives the deterministic pipeline against a golden
//! migration and returns a comparable result.
//!
//! Stage 1 (S3b) flow: load `MappingPlan` → run `Generator::generate` into
//! a temp directory → compare against `expected/`. Token cost = 0
//! placeholder; real Mapper integration arrives in S4.
//!
//! Constitution: Article III (evals are SoT), Article VI (deterministic
//! pipeline + byte-stable output makes the comparison meaningful).
//! Pattern: Scanner from P-04 — `Scanner::scan(root)` static-style on a
//! stateless struct. We bundle a Generator for slight efficiency.

use crate::errors::EvalError;
use crate::golden::GoldenMigration;
use crate::scorer::compare_directories;
use std::path::Path;
use std::time::Instant;
use tempfile::TempDir;
use terrashift_engine::generator::Generator;

/// Outcome of a single golden run. Stable shape — `token_cost_micros` is
/// 0 in S3b but populated in S4 once Mapper produces real costs.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub name: String,
    pub passed: bool,
    pub token_cost_micros: u64,
    pub wall_clock_ms: u32,
    /// Unified diff describing mismatches. `None` when `passed`.
    pub diff: Option<String>,
}

/// Aggregate report across the suite.
#[derive(Debug)]
pub struct SuiteReport {
    pub results: Vec<EvalResult>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub total_token_cost_micros: u64,
}

impl SuiteReport {
    /// Whether the suite as a whole passed. Used by CI as the merge gate.
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

/// The runner. Holds a `Generator` so suite runs share one instance.
pub struct EvalRunner {
    generator: Generator,
}

impl EvalRunner {
    /// Build a runner with the Stage 1 Generator (10 templates).
    pub fn new() -> Self {
        Self {
            generator: Generator::new(),
        }
    }

    /// Run one golden migration. Hermetic: uses fresh `TempDir`s for both
    /// `cwd` (backup tree) and `output_dir` (emitted .tf), so concurrent
    /// runs of the same fixture don't collide.
    pub fn run(&self, golden: &GoldenMigration) -> Result<EvalResult, EvalError> {
        let started = Instant::now();
        let cwd = TempDir::new().map_err(|e| EvalError::Io {
            path: golden.root.clone(),
            source: e,
        })?;
        let output_dir = TempDir::new().map_err(|e| EvalError::Io {
            path: golden.root.clone(),
            source: e,
        })?;

        // Run the deterministic Generator over the pre-curated MappingPlan.
        // S4: replace pre-curated plan with real Mapper invocation here.
        self.generator
            .generate(cwd.path(), output_dir.path(), &golden.mapping_plan)
            .map_err(|e| EvalError::Generator {
                name: golden.manifest.name.clone(),
                source: e,
            })?;

        let comparison = compare_directories(output_dir.path(), &golden.expected_target_dir)?;
        let elapsed = started.elapsed();

        Ok(EvalResult {
            name: golden.manifest.name.clone(),
            passed: comparison.passed,
            // Stage 1 placeholder — Article XII rule 1 plumbing without
            // active enforcement. S4 wires real LLM costs.
            token_cost_micros: 0,
            wall_clock_ms: u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX),
            diff: comparison.diff,
        })
    }

    /// Run the full suite at `suite_root`. Each fixture is loaded and
    /// run; load failures bubble up (Article IV — loud at load time
    /// rather than silently skipping).
    pub fn run_suite(&self, suite_root: &Path) -> Result<SuiteReport, EvalError> {
        let goldens = crate::golden::discover_suite(suite_root)?;
        let mut results = Vec::with_capacity(goldens.len());
        for g in &goldens {
            tracing::debug!(name = %g.manifest.name, "EvalRunner running fixture");
            results.push(self.run(g)?);
        }
        let passed = results.iter().filter(|r| r.passed).count();
        let total = results.len();
        let total_token_cost_micros = results.iter().map(|r| r.token_cost_micros).sum();
        tracing::info!(
            total,
            passed,
            failed = total - passed,
            total_token_cost_micros,
            "EvalRunner suite complete"
        );
        Ok(SuiteReport {
            results,
            total,
            passed,
            failed: total - passed,
            total_token_cost_micros,
        })
    }
}

impl Default for EvalRunner {
    fn default() -> Self {
        Self::new()
    }
}
