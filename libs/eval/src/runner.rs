// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

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

        // Schema presence check — fires only when the manifest declares
        // `required_schemas`. Pre-existing fixtures (no schemas declared)
        // skip immediately. RFC schema-source-migration §5.1.
        check_required_schemas(golden)?;

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

/// For each `required_schemas` entry, verify a corresponding compressed
/// schema file is present in `<suite_root>/.schema-cache/`. The expected
/// filename is `<provider>@<version>.json.zst`; compression itself isn't
/// checked here — Stage 1 only verifies existence. Decompression + content
/// validation lands in S4 alongside Mapper-driven eval runs.
///
/// `golden.root.parent()` resolves to the suite root for fixtures laid out
/// per the `terrashift-evals/<NNN_*>` convention. If `parent()` is `None`
/// (golden is at filesystem root), the check uses `golden.root` itself —
/// degenerate but doesn't panic.
fn check_required_schemas(golden: &GoldenMigration) -> Result<(), EvalError> {
    if golden.manifest.required_schemas.is_empty() {
        return Ok(());
    }
    let suite_root = golden.root.parent().unwrap_or(&golden.root);
    let cache_dir = suite_root.join(".schema-cache");
    for req in &golden.manifest.required_schemas {
        let schema_id = format!("{}@{}", req.provider, req.version);
        let file = cache_dir.join(format!("{schema_id}.json.zst"));
        if !file.exists() {
            return Err(EvalError::MissingSchema {
                fixture: golden.root.clone(),
                schema_id,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::golden::{GoldenManifest, RequiredSchema};
    use terrashift_engine::mapper::MappingPlan;
    use uuid::Uuid;

    fn synthetic_golden(
        suite_root: &Path,
        name: &str,
        required: Vec<RequiredSchema>,
    ) -> GoldenMigration {
        let root = suite_root.join(name);
        std::fs::create_dir_all(&root).expect("mkdir golden root");
        std::fs::create_dir_all(root.join("expected")).expect("mkdir expected");

        let manifest = GoldenManifest {
            name: name.to_string(),
            description: String::new(),
            source_provider: "aws".to_string(),
            target_provider: "aws".to_string(),
            token_cost_ceiling_micros: 0,
            articles: vec![],
            required_schemas: required,
        };

        let mapping_plan = MappingPlan {
            run_id: Uuid::new_v4(),
            source_provider: "aws".to_string(),
            target_provider: "aws".to_string(),
            resources: vec![],
        };

        GoldenMigration {
            root,
            manifest,
            source_tf: suite_root.join(name).join("source.tf"),
            mapping_plan,
            expected_target_dir: suite_root.join(name).join("expected"),
        }
    }

    #[test]
    fn check_passes_when_required_schemas_is_empty() {
        let suite = tempfile::tempdir().expect("tempdir");
        let g = synthetic_golden(suite.path(), "001_test", vec![]);
        check_required_schemas(&g).expect("empty required_schemas always passes");
    }

    #[test]
    fn check_fails_loud_when_schema_missing_from_cache() {
        let suite = tempfile::tempdir().expect("tempdir");
        let g = synthetic_golden(
            suite.path(),
            "002_test",
            vec![RequiredSchema {
                provider: "aws".to_string(),
                version: "5.30.0".to_string(),
            }],
        );
        let err = check_required_schemas(&g).expect_err("missing cache must fail");
        match err {
            EvalError::MissingSchema { schema_id, .. } => {
                assert_eq!(schema_id, "aws@5.30.0");
            }
            other => panic!("expected MissingSchema, got {other:?}"),
        }
    }

    #[test]
    fn check_passes_when_schema_file_present() {
        let suite = tempfile::tempdir().expect("tempdir");
        // Create the cache dir + the expected compressed file. We don't
        // need it to be valid zstd — Stage 1 only checks existence.
        let cache = suite.path().join(".schema-cache");
        std::fs::create_dir_all(&cache).expect("mkdir cache");
        std::fs::write(cache.join("aws@5.30.0.json.zst"), b"placeholder")
            .expect("write fake schema");

        let g = synthetic_golden(
            suite.path(),
            "003_test",
            vec![RequiredSchema {
                provider: "aws".to_string(),
                version: "5.30.0".to_string(),
            }],
        );
        check_required_schemas(&g).expect("present file must pass");
    }

    #[test]
    fn check_aggregates_first_missing_first() {
        let suite = tempfile::tempdir().expect("tempdir");
        let cache = suite.path().join(".schema-cache");
        std::fs::create_dir_all(&cache).expect("mkdir cache");
        std::fs::write(cache.join("aws@5.30.0.json.zst"), b"x").expect("write aws");
        // google deliberately absent.

        let mut g = synthetic_golden(suite.path(), "004_test", vec![]);
        g.manifest.required_schemas = vec![
            RequiredSchema {
                provider: "aws".to_string(),
                version: "5.30.0".to_string(),
            },
            RequiredSchema {
                provider: "google".to_string(),
                version: "5.40.2".to_string(),
            },
        ];

        let err = check_required_schemas(&g).expect_err("partial cache must fail loud");
        match err {
            EvalError::MissingSchema { schema_id, .. } => {
                assert_eq!(schema_id, "google@5.40.2");
            }
            other => panic!("expected MissingSchema(google@5.40.2), got {other:?}"),
        }
    }
}
