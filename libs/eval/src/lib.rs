//! Eval framework — golden migrations + token-cost regression gate.
//!
//! Pattern: stakpak_arch.md section 32 (CI matrix — eval suite is one of
//! gated CI checks).
//!
//! Constitution: Article III (evals are source of truth for AI safety),
//! Article XII rule 4 (CI regression gate: >30% token cost increase against
//! prior baseline blocks PR merge). Article XIII rule 2 (non-monotonic trim
//! breaks regression detection).
//!
//! Stage 1: 5 hand-curated golden migrations covering GCP→AWS scope.
//! Stage 1 expansion (P-15): grow to 10.
//!
//! Modules to be filled in by P-NN prompts (P-12):
//! - `golden.rs` — GoldenMigration { source_tf, expected_target_tf, expected_audit_summary }
//! - `runner.rs` — runs full pipeline against golden suite
//! - `scorer.rs` — diff-based scoring + token-cost tracking
//! - `regression.rs` — CI gate: >30% token cost OR any test fail = block
//! - `baseline.rs` — rolling baseline (median of last 10 green main runs)

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
