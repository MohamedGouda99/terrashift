//! Validator failure modes.
//!
//! Pattern: Terrashift-specific (Article III enforcement domain).
//! Constitution: Article IV (every failure named, no anonymous strings).

use thiserror::Error;

/// Top-level Validator failure modes. Distinct from per-resource
/// validation findings (which live in `ValidationReport.errors`) — these
/// are *infrastructure* failures (couldn't fetch the schema, etc.).
#[derive(Debug, Error)]
pub enum ValidatorError {
    /// Knowledge service could not produce the target schema. Could mean:
    /// schema not in cache and registry fetch failed, version unknown,
    /// transient network error.
    #[error("knowledge service: {0}")]
    Knowledge(#[from] terrashift_knowledge::KnowledgeError),
}
