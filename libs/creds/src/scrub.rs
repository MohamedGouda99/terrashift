// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `pre_llm_check` — outbound proactive scrubber.
//!
//! Pattern: thin wrapper over `terrashift_audit::scrubber::scan`
//! (single source of truth for gitleaks regex set + entropy heuristic).
//! The audit-writer's panic-on-detection (P-11 commit `4545065`) is
//! the *runtime* last-line-of-defence; this is the *outbound* layer
//! that catches the bug before it reaches the LLM.
//!
//! Constitution: Article XIII rule 5 (proactive layer of the
//! "don't bypass redaction" enforcement). Article V (error message
//! NEVER echoes the raw value — only pattern name + field path).

use crate::errors::CredsError;
use terrashift_audit::scrubber;

/// Scan a payload string for raw secrets. Returns `Ok(())` on clean
/// payload; `Err(SecretsDetected)` on any match. The error names the
/// pattern + field path; raw value is NEVER echoed.
pub fn pre_llm_check(field_path: &str, payload: &str) -> Result<(), CredsError> {
    if let Some(found) = scrubber::scan(field_path, payload) {
        return Err(CredsError::SecretsDetected {
            pattern_name: found.pattern_name,
            field_path: found.field_path,
        });
    }
    Ok(())
}

/// Scan multiple `(field_path, payload)` pairs in one call. Returns
/// the first match found; later fields aren't scanned (fail-fast is
/// fine here — the operator fixes one and re-runs).
pub fn pre_llm_check_many(fields: &[(&str, &str)]) -> Result<(), CredsError> {
    for (path, payload) in fields {
        pre_llm_check(path, payload)?;
    }
    Ok(())
}
