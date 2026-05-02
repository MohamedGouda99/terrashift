//! `ValidationReport` + per-finding shapes.
//!
//! Pattern: Terrashift-specific.
//! Constitution: Article III (errors block, names every offending attr —
//! `passed = errors.is_empty()`); Article IV (warnings surface but don't
//! silently mutate behaviour).

use std::fmt;

/// What `Validator::validate` returns. Aggregate (every problem found),
/// not fail-fast (clarify Q7) — operators see the full picture in one
/// pass instead of fix-loop debugging.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// `true` iff `errors.is_empty()`. Warnings don't flip this — that
    /// matches the spec's "errors blocking; warnings surfaced but don't
    /// block" line. A future `--strict` flag (S5+) elevates warnings.
    pub passed: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationReport {
    /// Construct from collected findings. Caller passes both vectors;
    /// `passed` is derived (never set independently).
    pub fn new(errors: Vec<ValidationError>, warnings: Vec<ValidationWarning>) -> Self {
        Self {
            passed: errors.is_empty(),
            errors,
            warnings,
        }
    }
}

/// Blocking findings — the migration cannot proceed.
///
/// Why these are blocking and not warnings: each one means
/// `terraform plan` will fail at Executor time with a deterministic
/// error. Surfacing them at Validator-time (before HCL emit) is what
/// makes Article III load-bearing — Stakpak agents would just plow
/// through and discover it later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The Mapper produced a `target_type` that isn't in the
    /// target-provider schema. Most likely cause: hallucinated
    /// resource type ("aws_buckets" for "aws_s3_bucket").
    UnknownResourceType { addr: String, target_type: String },
    /// The Mapper produced an attribute name that isn't in the
    /// resource schema. Most likely cause: hallucinated attribute
    /// ("aws_vpc.cidr_blocks" — plural — instead of "cidr_block").
    UnknownAttribute {
        addr: String,
        attr: String,
        target_type: String,
    },
    /// The schema marks an attribute `required: true` but the plan
    /// omits it. `terraform plan` would fail with
    /// `Error: Missing required argument`.
    MissingRequiredAttribute {
        addr: String,
        attr: String,
        target_type: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownResourceType { addr, target_type } => write!(
                f,
                "Article III violation: '{addr}' uses unknown target_type '{target_type}' (Mapper hallucinated a resource that doesn't exist in the target schema)"
            ),
            Self::UnknownAttribute { addr, attr, target_type } => write!(
                f,
                "Article III violation: '{addr}' (target_type='{target_type}') sets unknown attribute '{attr}' (Mapper hallucinated an attribute that doesn't exist in the schema)"
            ),
            Self::MissingRequiredAttribute { addr, attr, target_type } => write!(
                f,
                "Article IV violation: '{addr}' (target_type='{target_type}') is missing required attribute '{attr}'"
            ),
        }
    }
}

/// Non-blocking findings — surfaced to the user but don't fail the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationWarning {
    /// Attribute is `deprecated: Some(...)` in the schema. Still works
    /// in the current provider version but will be removed in a
    /// future version. Operator should plan a migration.
    DeprecatedAttribute {
        addr: String,
        attr: String,
        since: String,
    },
    /// Schema marks the attribute `computed: true` AND not `optional`
    /// (i.e., read-only). Setting it is permitted by `terraform plan`
    /// but indicates the Mapper got confused — those values are output
    /// by the provider, not input.
    SetComputedAttribute {
        addr: String,
        attr: String,
        target_type: String,
    },
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeprecatedAttribute { addr, attr, since } => write!(
                f,
                "deprecated: '{addr}' sets attribute '{attr}' (deprecated since: {since})"
            ),
            Self::SetComputedAttribute {
                addr,
                attr,
                target_type,
            } => write!(
                f,
                "computed: '{addr}' (target_type='{target_type}') sets read-only attribute '{attr}' — value will be ignored by terraform"
            ),
        }
    }
}
