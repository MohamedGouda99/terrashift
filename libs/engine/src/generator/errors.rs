//! Generator failure modes.
//!
//! Pattern: terrashift_plan.md §5; mirrors `libs/engine/src/scanner/errors.rs`
//! shape (one `thiserror` enum per concern, one Backup-specific sub-enum).
//! Constitution: Article IV (every failure mode is named, no silent fallthrough).

use std::path::PathBuf;
use thiserror::Error;

/// Top-level Generator failure modes.
#[derive(Debug, Error)]
pub enum GeneratorError {
    /// No Stage 1 template registered for `target_type`. Article IV: loud Err
    /// (we do NOT fall back to LLM in Stage 1; that path lives in S5+ per
    /// `specs/008-generator/clarify.md` Q5).
    #[error("template miss for target_type '{target_type}': no Stage 1 template registered")]
    TemplateMiss { target_type: String },

    /// HCL serialization failed. Wraps the underlying `hcl::Error`.
    #[error("HCL serialization failed: {0}")]
    Hcl(#[from] hcl::Error),

    /// Filesystem I/O failure. `path` names the file that triggered it.
    #[error("filesystem I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Wrapped backup-stage failure (move/restore).
    #[error(transparent)]
    Backup(#[from] BackupError),

    /// Wrapped audit-store failure (rare but possible — disk full, schema mismatch).
    #[error("audit append failed: {0}")]
    Audit(String),

    /// Caller passed a path that isn't a directory.
    #[error("output_dir '{0}' is not a directory")]
    InvalidOutputDir(PathBuf),

    /// Mapper helper returned a missing-or-mistyped attribute.
    #[error(transparent)]
    Lookup(#[from] crate::mapper::MapperLookupError),

    /// `AttributeValue::Reference("")` — the Mapper produced an empty
    /// reference. Cannot emit as raw HCL (would silently become `attr = ""`,
    /// masking the upstream bug). Article IV: fail loudly.
    #[error("empty reference value: Mapper emitted AttributeValue::Reference(\"\") — Article IV bug upstream")]
    EmptyReference,

    /// Aggregate failure across `Generator::rollback`. We attempt every
    /// per-file restore and report all failures together so partial rollback
    /// state is observable, not lost. Article V: reversibility must hold
    /// even in the partial-failure case.
    #[error(
        "rollback partial failure: {failures} of {total} files failed to restore: {details:?}"
    )]
    RollbackPartial {
        total: usize,
        failures: usize,
        details: Vec<String>,
    },
}

/// Backup-and-restore failures. Separate enum so callers can distinguish
/// "the backup step itself broke" from "the file write broke."
#[derive(Debug, Error)]
pub enum BackupError {
    /// Asked to back up something that isn't there.
    #[error("source path '{0}' does not exist (cannot back up)")]
    SourceMissing(PathBuf),

    /// Could not create the backup directory.
    #[error("create backup dir '{path}': {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Move failed (and the EXDEV copy+remove fallback also failed, if attempted).
    #[error("rename '{src}' → '{dst}': {source}")]
    Move {
        src: PathBuf,
        dst: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Restore (rollback) failed.
    #[error("restore '{src}' → '{dst}': {source}")]
    Restore {
        src: PathBuf,
        dst: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
