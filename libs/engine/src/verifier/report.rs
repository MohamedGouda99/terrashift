// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `VerifierReport` + per-finding shapes.
//!
//! Pattern: mirrors `validator::report::ValidationReport`.
//! Constitution: Article IV (errors block, named; warnings surface but
//! don't silently mutate behaviour); Article IX (errors plus warnings
//! together form the auditable post-apply record).

use std::fmt;
use uuid::Uuid;

/// What `Verifier::verify` returns. Aggregate (every drift finding),
/// not fail-fast — operators see the full picture in one pass.
#[derive(Debug, Clone)]
pub struct VerifierReport {
    /// `true` iff `errors.is_empty()`. Warnings don't flip this — that
    /// matches the spec's "errors blocking; warnings surfaced but don't
    /// block" line. A future `--strict` flag (S5+) elevates warnings.
    pub passed: bool,
    /// The migration this report is for. Verifier copies it from
    /// `MappingPlan.run_id`; downstream systems index audit entries on it.
    pub run_id: Uuid,
    /// How many resources the plan said should be in state.
    pub expected_count: usize,
    /// How many resources actually are in state (after recursive
    /// `child_modules` flatten).
    pub actual_count: usize,
    pub errors: Vec<VerifierIssue>,
    pub warnings: Vec<VerifierWarning>,
}

impl VerifierReport {
    pub fn new(
        run_id: Uuid,
        expected_count: usize,
        actual_count: usize,
        errors: Vec<VerifierIssue>,
        warnings: Vec<VerifierWarning>,
    ) -> Self {
        Self {
            passed: errors.is_empty(),
            run_id,
            expected_count,
            actual_count,
            errors,
            warnings,
        }
    }
}

/// Blocking findings — the migration didn't land cleanly.
///
/// Why these are blocking and not warnings: each one means an `apply`
/// didn't do what the plan said, OR the operator hand-edited state in a
/// way that diverges from the plan. Surfacing them at Verifier-time
/// (between apply and "migration complete" notification) is what makes
/// post-apply checking load-bearing — without this gate we ship an
/// apply-and-pray pipeline (Article IV violation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifierIssue {
    /// The plan listed this resource but state doesn't contain it.
    /// Most common cause: partial apply (network blip, IAM denied).
    MissingResource {
        address: String,
        resource_type: String,
    },
    /// Same address in plan and state, but different type. Most common
    /// cause: hand-edited state file or a previously-failed migration's
    /// state remnants.
    TypeMismatch {
        address: String,
        expected_type: String,
        actual_type: String,
    },
}

impl fmt::Display for VerifierIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingResource {
                address,
                resource_type,
            } => write!(
                f,
                "Article IV violation: '{address}' (type='{resource_type}') is in the plan but not in state — apply may have partially failed"
            ),
            Self::TypeMismatch {
                address,
                expected_type,
                actual_type,
            } => write!(
                f,
                "Article IV violation: '{address}' has type '{actual_type}' in state but plan expected '{expected_type}' — state has been hand-edited or rolled back"
            ),
        }
    }
}

/// Non-blocking findings — surfaced to the user but don't fail the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifierWarning {
    /// State has a resource the plan doesn't know about. Common when an
    /// operator manually adds resources to a Terrashift-migrated stack;
    /// also the signal for drift-detection workflows.
    ExtraResource {
        address: String,
        resource_type: String,
    },
}

impl fmt::Display for VerifierWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtraResource {
                address,
                resource_type,
            } => write!(
                f,
                "extra resource: '{address}' (type='{resource_type}') is in state but not in the plan — possibly hand-added or pre-existing"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_passed_iff_errors_empty() {
        let run_id = Uuid::new_v4();
        let with_err = VerifierReport::new(
            run_id,
            1,
            0,
            vec![VerifierIssue::MissingResource {
                address: "aws_vpc.x".to_string(),
                resource_type: "aws_vpc".to_string(),
            }],
            vec![],
        );
        assert!(!with_err.passed);

        let with_warn_only = VerifierReport::new(
            run_id,
            1,
            2,
            vec![],
            vec![VerifierWarning::ExtraResource {
                address: "aws_vpc.y".to_string(),
                resource_type: "aws_vpc".to_string(),
            }],
        );
        assert!(with_warn_only.passed);
    }

    #[test]
    fn issue_display_names_address_and_type() {
        let issue = VerifierIssue::MissingResource {
            address: "aws_vpc.main".to_string(),
            resource_type: "aws_vpc".to_string(),
        };
        let s = format!("{issue}");
        assert!(s.contains("aws_vpc.main"));
        assert!(s.contains("aws_vpc"));
        assert!(s.contains("Article IV"));
    }

    #[test]
    fn type_mismatch_display_shows_both_types() {
        let issue = VerifierIssue::TypeMismatch {
            address: "aws_vpc.main".to_string(),
            expected_type: "aws_vpc".to_string(),
            actual_type: "aws_vpc_v2".to_string(),
        };
        let s = format!("{issue}");
        assert!(s.contains("aws_vpc"));
        assert!(s.contains("aws_vpc_v2"));
    }

    #[test]
    fn warning_display_distinguishes_from_error() {
        let warn = VerifierWarning::ExtraResource {
            address: "aws_s3_bucket.uploads".to_string(),
            resource_type: "aws_s3_bucket".to_string(),
        };
        let s = format!("{warn}");
        assert!(s.contains("extra resource"));
        assert!(!s.contains("Article IV"));
    }
}
