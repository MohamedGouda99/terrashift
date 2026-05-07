// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Pre-write secret scrubber — the LAST line of defence (Article XIII rule 5).
//!
//! Pattern: terrashift_plan.md §6.X ("the writer is the LAST line of defence —
//! panics loudly").
//! Constitution: Article XIII rule 5 (no proxy bypass). The audit writer
//! checks every string field; on detection it returns `SecretsDetected` and
//! the AuditStore.append() converts that into a panic per §6.X discipline.
//!
//! ## Stage 1 patterns (minimal but sufficient)
//!
//! Per clarify Q5: enough to catch the most common real leaks. Full gitleaks
//! rule set (~60 patterns) arrives in S30 (security-auditor sub-agent uses it).

use regex::Regex;
use std::sync::OnceLock;

/// Names of the patterns we check, exposed to error messages so reviewers
/// know WHAT was detected without seeing the raw value.
///
/// **Note on length:** the four named entries above the heuristic line are
/// the regex-backed patterns; `"high_entropy_token"` is the inline heuristic
/// at the bottom of `scan()`. They share a name namespace but are checked
/// by different code paths. Do NOT zip this constant with `patterns()` —
/// it has one fewer entry by design.
pub const PATTERN_NAMES: &[&str] = &[
    // Regex patterns (in order returned by `patterns()`):
    "aws_access_key",
    "gcp_api_key",
    "github_pat",
    "private_key_header",
    // Heuristic — NOT in `patterns()`:
    "high_entropy_token",
];

/// Compile patterns once at first use. Panics at startup if any pattern
/// fails to compile — that's a literal-string-typo bug, not a data-driven
/// failure, and tests in this module exercise every pattern.
#[allow(clippy::expect_used)]
fn patterns() -> &'static Vec<(&'static str, Regex)> {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            (
                "aws_access_key",
                Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("aws_access_key regex"),
            ),
            (
                "gcp_api_key",
                Regex::new(r"\bAIza[0-9A-Za-z_\-]{35}\b").expect("gcp_api_key regex"),
            ),
            (
                "github_pat",
                Regex::new(r"\bghp_[A-Za-z0-9]{36}\b").expect("github_pat regex"),
            ),
            (
                "private_key_header",
                Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")
                    .expect("private_key_header regex"),
            ),
        ]
    })
}

/// What the scrubber found. Pattern name (per PATTERN_NAMES); raw value is
/// NEVER echoed in the result (it would re-leak the very thing we caught).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubMatch {
    pub pattern_name: String,
    pub field_path: String,
}

/// Scan one string for secret patterns. Returns the first match.
pub fn scan(field_path: &str, value: &str) -> Option<ScrubMatch> {
    for (name, re) in patterns() {
        if re.is_match(value) {
            return Some(ScrubMatch {
                pattern_name: (*name).to_string(),
                field_path: field_path.to_string(),
            });
        }
    }

    // SHA-256 hex output: 64 chars, hex-only (case-insensitive). Used by
    // AuditPayload::SchemaCapture.sha256 and Generator FileOperation hashes.
    // Exempt from the high-entropy heuristic — these are content-addressable
    // identifiers we deliberately store and a known-shape false positive.
    if value.len() == 64
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c) || ('A'..='F').contains(&c))
    {
        return None;
    }

    // High-entropy heuristic: if a token is ≥40 chars of base64-ish charset,
    // it's likely a secret. Tunable; false-positive rate is the trade-off.
    if value.len() >= 40
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '_' || c == '-'
        })
        && value.chars().filter(|c| c.is_ascii_uppercase()).count() >= 4
        && value.chars().filter(|c| c.is_ascii_digit()).count() >= 4
    {
        return Some(ScrubMatch {
            pattern_name: "high_entropy_token".to_string(),
            field_path: field_path.to_string(),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_aws_key() {
        let r = scan("payload.target", "AKIAIOSFODNN7EXAMPLE");
        assert!(r.is_some());
        assert_eq!(r.unwrap().pattern_name, "aws_access_key");
    }

    #[test]
    fn detects_gcp_key() {
        let r = scan("payload.target", "AIzaSyDdI0hCZtE6vySjMm-WEfRq3CPzqKqqsHI");
        assert!(r.is_some());
        assert_eq!(r.unwrap().pattern_name, "gcp_api_key");
    }

    #[test]
    fn detects_github_pat() {
        let r = scan("payload.target", "ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(r.is_some());
        assert_eq!(r.unwrap().pattern_name, "github_pat");
    }

    #[test]
    fn detects_private_key_header() {
        let r = scan(
            "payload.message",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQE...",
        );
        assert!(r.is_some());
        assert_eq!(r.unwrap().pattern_name, "private_key_header");
    }

    #[test]
    fn passes_clean_text() {
        assert!(scan("payload.target", "aws_vpc.main.id").is_none());
        assert!(scan("payload.tool_name", "scan").is_none());
        assert!(scan("payload.cred_ref", "{{secret:aws-prod-deploy}}").is_none());
    }

    #[test]
    fn high_entropy_heuristic() {
        // 48-char base64-ish with mix of upper + digit
        let r = scan(
            "payload.target",
            "AbcDefGhi1234567jklMNopQRSt9876UVWxyz0aBcDeFg12",
        );
        assert!(r.is_some());
        assert_eq!(r.unwrap().pattern_name, "high_entropy_token");
    }

    #[test]
    fn sha256_hex_is_exempt_from_heuristic() {
        // Real SHA-256 hex output (64 lowercase hex chars). Has plenty of
        // digits and could superficially look high-entropy, but is a known
        // content-addressable identifier shape and must not trigger.
        let r = scan(
            "payload.sha256",
            "a1f7e21d8c3b0429f5e8d6c4b8a9f2e1d3c5b7a9e0d2f4c6b8a1d3e5f7c9b1d3",
        );
        assert!(r.is_none(), "lowercase SHA-256 hex must be exempt");

        // Same length, mixed case (also a valid hex shape per RFC 4648).
        let r = scan(
            "payload.sha256",
            "A1F7E21D8C3B0429F5E8D6C4B8A9F2E1D3C5B7A9E0D2F4C6B8A1D3E5F7C9B1D3",
        );
        assert!(r.is_none(), "uppercase SHA-256 hex must be exempt");

        // Same length, but contains a non-hex letter — exemption must NOT
        // apply; falls through to the regular heuristic.
        let r = scan(
            "payload.sha256",
            "G1F7E21D8C3B0429F5E8D6C4B8A9F2E1D3C5B7A9E0D2F4C6B8A1D3E5F7C9B1D3",
        );
        assert!(
            r.is_some(),
            "non-hex char must defeat the SHA-256 exemption"
        );
    }
}
