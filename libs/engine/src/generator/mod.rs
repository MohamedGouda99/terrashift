// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Generator — deterministic HCL emit + reversible file ops.
//!
//! Fourth component of the migration pipeline (HLD-2 box 4). Takes a
//! validated `MappingPlan` (from Mapper, S4) and produces target `.tf`
//! files in the output directory. Pure deterministic Rust — NO LLM in
//! the happy path (Article I).
//!
//! Pattern: the architecture reference §28 (reversible file operations).
//! Source: the reference codebase (see ATTRIBUTIONS.md)
//!         (verbatim move-to-backup; we add EXDEV copy+remove fallback).
//! Constitution: Article I (deterministic, not an agent),
//!               Article IV (loud failures on template miss),
//!               Article V (reversible — every overwrite backed up),
//!               Article IX (backups archival, never auto-deleted),
//!               Article XIII rule 3 (no unwrap/expect/string-slice).
//!
//! ## Public API
//!
//! ```no_run
//! use terrashift_engine::generator::Generator;
//! use terrashift_engine::mapper::MappingPlan;
//! use std::path::Path;
//! use uuid::Uuid;
//!
//! # fn doctest(plan: MappingPlan, output_dir: &Path) -> Result<(), terrashift_engine::generator::GeneratorError> {
//! let cwd = std::env::current_dir().map_err(|e| terrashift_engine::generator::GeneratorError::Io {
//!     path: ".".into(),
//!     source: e,
//! })?;
//! let gen = Generator::new();
//! let artifacts = gen.generate(&cwd, output_dir, &plan)?;
//! println!("wrote {} files, {} backups", artifacts.files.len(), artifacts.backups_created);
//! # Ok(())
//! # }
//! ```

pub mod backup;
pub mod emitter;
pub mod errors;
pub mod templates;

pub use errors::{BackupError, GeneratorError};
pub use templates::TemplateRegistry;

use crate::mapper::MappingPlan;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use terrashift_knowledge::ProviderSchema;

/// Generator facade.
///
/// Stateless apart from the template registry and an optional audit-store
/// hook; safe to share between threads (registry is `Sync` because all
/// template fns are pure). Construct once per process; reuse across runs.
pub struct Generator {
    registry: TemplateRegistry,
}

impl Generator {
    /// Build with the Stage 1 hand-curated template set only. Use in tests
    /// or when no schema cache is available. For production migration runs,
    /// prefer [`Generator::with_schemas`] so every resource type in the
    /// loaded schemas becomes emittable.
    pub fn new() -> Self {
        Self {
            registry: TemplateRegistry::stage1(),
        }
    }

    /// Build with hand-curated templates PLUS schema-derived fallback for
    /// every resource type in `schemas`. This is the production constructor
    /// — at startup the migration pipeline enumerates loaded provider
    /// schemas (typically aws + azurerm + google) and passes them here. The
    /// resulting registry covers thousands of resource types without any
    /// hand-curated source enumeration (Article XIII rule 8).
    pub fn with_schemas(schemas: impl IntoIterator<Item = Arc<ProviderSchema>>) -> Self {
        Self {
            registry: TemplateRegistry::with_schemas(schemas),
        }
    }

    /// Run the Generator: emit every resource in `plan` into `output_dir`.
    ///
    /// Side effects:
    /// 1. Sorts resources by `target_addr` (Article VI — deterministic).
    /// 2. Groups by `target_type` → one file per type.
    /// 3. For each existing file at the target path: moves to
    ///    `<cwd>/.terrashift/runs/{run_id}/backups/{op_uuid}/<filename>`
    ///    (mirrors `the reference codebase (see ATTRIBUTIONS.md)`).
    /// 4. Writes the new content.
    ///
    /// `cwd` is where the `.terrashift/` backup tree lives. Tests pass a
    /// `tempfile::TempDir` path; production code passes `std::env::current_dir()`.
    pub fn generate(
        &self,
        cwd: &Path,
        output_dir: &Path,
        plan: &MappingPlan,
    ) -> Result<GeneratedArtifacts, GeneratorError> {
        tracing::debug!(
            run_id = %plan.run_id,
            source = %plan.source_provider,
            target = %plan.target_provider,
            n_resources = plan.resources.len(),
            "Generator::generate start"
        );

        let emitted = emitter::emit(cwd, output_dir, plan, &self.registry)?;

        let backups_created = emitted.iter().filter(|f| f.overwrote).count();

        tracing::info!(
            run_id = %plan.run_id,
            files_written = emitted.len(),
            backups_created,
            "Generator::generate done"
        );

        Ok(GeneratedArtifacts {
            files: emitted.iter().map(|f| f.path.clone()).collect(),
            backups: emitted
                .iter()
                .filter_map(|f| f.backup_path.clone())
                .collect(),
            backups_created,
            emitted,
        })
    }

    /// Roll back a previously-generated artifact set: restore every
    /// backup file to its original path; delete every file that was
    /// freshly created.
    pub fn rollback(&self, artifacts: &GeneratedArtifacts) -> Result<(), GeneratorError> {
        emitter::rollback_emitted(&artifacts.emitted)
    }

    /// Total templates registered. Used by tests + the Stage-1 exit gate.
    pub fn template_count(&self) -> usize {
        self.registry.len()
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a `Generator::generate` call. Caller-driven audit emission
/// uses the `emitted` list directly to construct `AuditPayload::FileOperation`
/// entries (one per file).
#[derive(Debug)]
pub struct GeneratedArtifacts {
    /// Every `.tf` file written by this run.
    pub files: Vec<PathBuf>,
    /// Backup paths for files that were overwritten.
    pub backups: Vec<PathBuf>,
    /// Convenience counter — `backups.len()` minus any greenfield writes.
    pub backups_created: usize,
    /// Per-file detail (kept for `rollback`).
    pub emitted: Vec<emitter::EmittedFile>,
}
