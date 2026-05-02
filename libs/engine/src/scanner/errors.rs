//! Scanner errors.
//!
//! Pattern: terrashift_plan.md §5 (HLD-2 pipeline component 1).
//! Constitution: Article IV (failures must be loud).

use std::path::PathBuf;
use thiserror::Error;

/// Scanner failures. Each variant carries enough context for a human to
/// localize the problem without re-running.
#[derive(Debug, Error)]
pub enum ScannerError {
    /// I/O error while walking the directory or opening a file.
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// HCL parse failure (syntax error, unexpected EOF, etc.).
    #[error("hcl parse error in {path}: {message}")]
    Parse { path: PathBuf, message: String },

    /// Stage 1 doesn't support this Terraform feature; loud-fail per Article IV
    /// rather than silently skip. See pre-flight.md / terrashift_plan.md §11.
    #[error(
        "unsupported feature '{feature}' in {path} (line {line}): {reason}\n\
         Stage 1 supports a focused subset; this feature ships in Stage 5. \
         To proceed, refactor the source to avoid this feature, or wait for Stage 5."
    )]
    UnsupportedFeature {
        feature: String,
        path: PathBuf,
        line: usize,
        reason: String,
    },

    /// Block had unexpected label structure (e.g., resource without 2 labels).
    #[error("malformed block in {path}: {message}")]
    MalformedBlock { path: PathBuf, message: String },
}
