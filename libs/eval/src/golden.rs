// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Golden migration loading + suite discovery.
//!
//! A golden migration is one fixture directory containing:
//! - `manifest.toml` (required) — `GoldenManifest` per spec
//! - `source.tf` (required) — source-cloud HCL (informational for S3b;
//!   in S4 the Mapper consumes it)
//! - `mapping_plan.json` (required) — pre-curated `MappingPlan`
//!   (Mapper output emulation until S4)
//! - `expected/` (required) — expected target `.tf` files, one per
//!   `target_type` group
//!
//! Pattern: TOML manifest convention from terrashift_plan.md §6.X
//! (`~/.terrashift/config.toml`). JSON for machine-emitted plan data.

use crate::errors::EvalError;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use terrashift_engine::mapper::MappingPlan;

/// One fixture's metadata. Each golden directory has a `manifest.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub source_provider: String,
    pub target_provider: String,
    /// Stage 1: 0 placeholder. S4 sets real ceilings once Mapper produces
    /// LLM token costs (Article XII rule 1).
    #[serde(default)]
    pub token_cost_ceiling_micros: u64,
    /// Which Constitution articles this golden validates. Used by the
    /// SuiteReport to surface coverage gaps.
    #[serde(default)]
    pub articles: Vec<u8>,
    /// Provider schemas required by the eval framework's Validator path.
    /// Each entry is verified present in the test-fixture schema cache
    /// (`<suite_root>/.schema-cache/`) BEFORE the Generator runs — a
    /// missing schema produces `EvalError::MissingSchema`, not a silent
    /// pass. Empty by default; pre-existing fixtures load unchanged.
    /// RFC schema-source-migration §5.1.
    #[serde(default)]
    pub required_schemas: Vec<RequiredSchema>,
}

/// One schema requirement in `GoldenManifest::required_schemas`. Currently
/// match-by-equality on `(provider, version)`; constraint expansion (e.g.
/// `~> 5.30`) lands in S4 alongside `Mapper`-driven eval runs.
#[derive(Debug, Clone, Deserialize)]
pub struct RequiredSchema {
    pub provider: String,
    pub version: String,
}

/// Loaded golden migration — paths verified, manifest parsed, plan parsed.
#[derive(Debug, Clone)]
pub struct GoldenMigration {
    pub root: PathBuf,
    pub manifest: GoldenManifest,
    pub source_tf: PathBuf,
    pub mapping_plan: MappingPlan,
    pub expected_target_dir: PathBuf,
}

/// Load a single golden migration directory. Returns `Err` if any
/// required file is missing or fails to parse — Article IV: loud failure
/// at load time, not at test time.
pub fn load_golden(root: &Path) -> Result<GoldenMigration, EvalError> {
    if !root.is_dir() {
        return Err(EvalError::Io {
            path: root.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "golden root is not a directory",
            ),
        });
    }

    let manifest_path = root.join("manifest.toml");
    if !manifest_path.is_file() {
        return Err(EvalError::MissingFixtureFile {
            path: root.to_path_buf(),
            missing: "manifest.toml",
        });
    }

    let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|e| EvalError::Io {
        path: manifest_path.clone(),
        source: e,
    })?;
    let manifest: GoldenManifest =
        toml::from_str(&manifest_text).map_err(|e| EvalError::ManifestParse {
            path: manifest_path.clone(),
            source: Box::new(e),
        })?;

    let source_tf = root.join("source.tf");
    if !source_tf.is_file() {
        return Err(EvalError::MissingFixtureFile {
            path: root.to_path_buf(),
            missing: "source.tf",
        });
    }

    let plan_path = root.join("mapping_plan.json");
    if !plan_path.is_file() {
        return Err(EvalError::MissingFixtureFile {
            path: root.to_path_buf(),
            missing: "mapping_plan.json",
        });
    }
    let plan_text = std::fs::read_to_string(&plan_path).map_err(|e| EvalError::Io {
        path: plan_path.clone(),
        source: e,
    })?;
    let mapping_plan: MappingPlan =
        serde_json::from_str(&plan_text).map_err(|e| EvalError::MappingPlanParse {
            path: plan_path.clone(),
            source: Box::new(e),
        })?;

    let expected_target_dir = root.join("expected");
    if !expected_target_dir.is_dir() {
        return Err(EvalError::MissingFixtureFile {
            path: root.to_path_buf(),
            missing: "expected/",
        });
    }

    Ok(GoldenMigration {
        root: root.to_path_buf(),
        manifest,
        source_tf,
        mapping_plan,
        expected_target_dir,
    })
}

/// Walk `suite_root` one level deep, load every subdirectory that has a
/// `manifest.toml`. Skips dotfiles and files. Returns goldens sorted by
/// directory name (so `001_*` comes before `002_*` — matters for the
/// SuiteReport ordering).
pub fn discover_suite(suite_root: &Path) -> Result<Vec<GoldenMigration>, EvalError> {
    if !suite_root.is_dir() {
        return Err(EvalError::InvalidSuiteRoot(suite_root.to_path_buf()));
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(suite_root)
        .map_err(|e| EvalError::Io {
            path: suite_root.to_path_buf(),
            source: e,
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && !entry.file_name().to_string_lossy().starts_with('.')
        })
        .filter(|entry| entry.path().join("manifest.toml").is_file())
        .map(|entry| entry.path())
        .collect();
    entries.sort();

    entries.iter().map(|p| load_golden(p)).collect()
}
