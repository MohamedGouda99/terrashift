//! HCL emit + file write — composes blocks into `Body`, serializes, writes.
//!
//! Pattern: the architecture reference §28 (file ops via the backup wrapper).
//! Constitution: Article VI (deterministic — sort + stable serialization),
//!               Article XIII rule 3 (no unwrap/expect in production).

use crate::generator::backup::move_to_backup;
use crate::generator::errors::GeneratorError;
use crate::generator::templates::{self, TemplateRegistry};
use crate::mapper::{MappedResource, MappingPlan};
use std::path::{Path, PathBuf};

/// One unit of write work: render the blocks for `target_type` to
/// `<output_dir>/<target_type>.tf`. Returns the file path written + the
/// backup path if one was created (for audit emission).
#[derive(Debug, Clone)]
pub struct EmittedFile {
    pub path: PathBuf,
    pub backup_path: Option<PathBuf>,
    /// True if a backup was created (i.e., an existing file was overwritten).
    pub overwrote: bool,
}

/// Render every group in `plan` into the corresponding `.tf` files under
/// `output_dir`. Existing files are moved to `.terrashift/runs/{run_id}/backups/`
/// before overwrite. Caller drives audit emission via the returned
/// `EmittedFile` list.
///
/// `cwd` is the working directory (for placing the `.terrashift/` backup
/// tree). the reference uses `std::env::current_dir()` directly
/// (`the reference codebase (see ATTRIBUTIONS.md)`); we accept it as a
/// parameter so tests can isolate via `tempfile`.
pub fn emit(
    cwd: &Path,
    output_dir: &Path,
    plan: &MappingPlan,
    registry: &TemplateRegistry,
) -> Result<Vec<EmittedFile>, GeneratorError> {
    if !output_dir.is_dir() {
        return Err(GeneratorError::InvalidOutputDir(output_dir.to_path_buf()));
    }

    let sorted = templates::sort_resources(&plan.resources);
    let groups = templates::group_by_target_type(&sorted);

    let mut emitted = Vec::with_capacity(groups.len());
    for (target_type, resources) in groups {
        let template = registry
            .template_for(&target_type)
            .ok_or(GeneratorError::TemplateMiss {
                target_type: target_type.clone(),
            })?;

        let blocks: Result<Vec<_>, _> = resources.iter().map(|r| template(r)).collect();
        let body = templates::body_from_blocks(blocks?);
        let serialized = hcl::to_string(&body).map_err(GeneratorError::Hcl)?;

        let target_path = output_dir.join(format!("{}.tf", target_type));
        let backup_path = if target_path.exists() {
            Some(move_to_backup(cwd, plan.run_id, &target_path)?)
        } else {
            None
        };

        std::fs::write(&target_path, &serialized).map_err(|e| GeneratorError::Io {
            path: target_path.clone(),
            source: e,
        })?;

        emitted.push(EmittedFile {
            path: target_path,
            overwrote: backup_path.is_some(),
            backup_path,
        });
    }

    Ok(emitted)
}

/// Render a single `MappedResource` to a string (without writing to disk).
/// Used by tests that want to inspect the emit output without filesystem
/// side effects.
pub fn render_one(
    resource: &MappedResource,
    registry: &TemplateRegistry,
) -> Result<String, GeneratorError> {
    let template =
        registry
            .template_for(&resource.target_type)
            .ok_or(GeneratorError::TemplateMiss {
                target_type: resource.target_type.clone(),
            })?;
    let block = template(resource)?;
    let body = templates::body_from_blocks(vec![block]);
    hcl::to_string(&body).map_err(GeneratorError::Hcl)
}

/// Convenience for callers that don't have a `MappingPlan` — render a slice
/// of resources to one big string. Used by `Generator::dry_run` (Stage 5+)
/// and by deterministic regression tests.
#[allow(dead_code)]
pub fn render_many(
    resources: &[MappedResource],
    registry: &TemplateRegistry,
) -> Result<String, GeneratorError> {
    let sorted = templates::sort_resources(resources);
    let mut blocks = Vec::with_capacity(sorted.len());
    for r in sorted {
        let template =
            registry
                .template_for(&r.target_type)
                .ok_or(GeneratorError::TemplateMiss {
                    target_type: r.target_type.clone(),
                })?;
        blocks.push(template(r)?);
    }
    let body = templates::body_from_blocks(blocks);
    hcl::to_string(&body).map_err(GeneratorError::Hcl)
}

/// Helper reused by the rollback path in `mod.rs` — given a list of emitted
/// files (plus their backups), restore each one. Used after a downstream
/// failure (e.g., Validator rejects post-emit).
///
/// **Idempotent / partial-failure aware** (Article V): every per-file
/// restore is attempted regardless of upstream errors. Failures are
/// aggregated into `GeneratorError::RollbackPartial` so callers can
/// observe which files restored and which didn't. Stopping on first
/// failure would leave the system in a half-restored state with no
/// clear path forward.
#[allow(dead_code)]
pub fn rollback_emitted(emitted: &[EmittedFile]) -> Result<(), GeneratorError> {
    let mut details: Vec<String> = Vec::new();
    for file in emitted {
        let result: Result<(), GeneratorError> = if let Some(backup) = &file.backup_path {
            crate::generator::backup::restore_from_backup(backup, &file.path)
                .map_err(GeneratorError::Backup)
        } else if file.path.exists() {
            // Greenfield write — best we can do is delete it.
            std::fs::remove_file(&file.path).map_err(|e| GeneratorError::Io {
                path: file.path.clone(),
                source: e,
            })
        } else {
            Ok(())
        };
        if let Err(e) = result {
            details.push(format!("{}: {}", file.path.display(), e));
        }
    }
    if details.is_empty() {
        Ok(())
    } else {
        Err(GeneratorError::RollbackPartial {
            total: emitted.len(),
            failures: details.len(),
            details,
        })
    }
}
