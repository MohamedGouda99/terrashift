//! Article XII rule 4 — token-cost regression gate.
//!
//! Reads a committed `eval-baseline.json` snapshot of expected
//! per-fixture token costs, compares it to a fresh `SuiteReport` from
//! `EvalRunner::run_suite`, and produces a `RegressionReport` with a
//! `Pass`/`Fail` verdict. CI uses the verdict to decide whether to
//! merge a PR.
//!
//! Pattern: terrashift_plan.md §6.X (token economy framing); R6
//! remediation ticket from the Stage 1 P-16 stage-gate review.
//! Constitution: Article XII rule 4 (>30% regression blocks merge),
//! Article IV (loud failures — coverage drift surfaces, missing
//! fixtures named), Article XIII rule 3 (no panics in production).
//!
//! ## Stage 1 vs S4-close
//!
//! Stage 1: every fixture's `token_cost_micros == 0` (no LLM yet).
//! Baseline file ships with all-zero values. The gate runs in
//! `BaselineMode::Lenient` so a future first-non-zero report (when
//! S4-close wires the real Mapper) is accepted as a baseline seed.
//! After S4-close, CI flips to `BaselineMode::Strict` and the >30%
//! gate is fully enforced.
//!
//! See `specs/r06-eval-baseline-gate/spec.md` for the full design.

use crate::errors::EvalError;
use crate::runner::SuiteReport;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Article XII rule 4 — the regression threshold. A `>30%` increase in
/// `total_token_cost_micros` against the committed baseline blocks
/// merge in `BaselineMode::Strict`.
///
/// 30 chosen because it's a generous tolerance for prompt-tuning churn
/// without letting cost drift go undetected. Stakpak's CI uses 25%
/// (refs/stakpak — informal); we picked 30% in `Constitution.md`
/// Article XII rule 4 to give Mapper prompt iteration breathing room
/// during Stage 1-2.
pub const REGRESSION_THRESHOLD_PCT: f64 = 30.0;

/// Committed snapshot of expected per-fixture token costs. Lives at
/// `terrashift-evals/eval-baseline.json` (same dir as fixtures so a
/// future `git mv terrashift-evals/ <private-repo>/` carries baseline
/// along — per specs/012-eval-framework/clarify.md Q1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    /// Sum across all fixtures. Stage 1: 0. S4-close updates this.
    pub total_token_cost_micros: u64,
    /// Per-fixture detail. Key is the fixture's `manifest.name`.
    pub per_fixture: BTreeMap<String, FixtureBaseline>,
    /// When the baseline was last refreshed. Operator-set; not used
    /// for gate decisions but valuable for auditing how stale the
    /// baseline is.
    pub last_refreshed_iso: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixtureBaseline {
    pub token_cost_micros: u64,
}

/// How strictly to interpret the baseline.
///
/// - **Lenient**: an all-zero baseline accepts a first-non-zero report
///   as a seed (returns `Pass` + `first_baseline_seed_recommended`).
///   Used in Stage 1 so we don't fail CI before S4-close lands.
/// - **Strict**: enforces `delta_pct <= REGRESSION_THRESHOLD_PCT`
///   in all cases; refuses a first-non-zero seed (operator must
///   refresh the baseline file consciously). Used after S4-close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineMode {
    Lenient,
    Strict,
}

/// What the gate decides. CI exits non-zero on `Fail`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegressionVerdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegressionReport {
    pub verdict: RegressionVerdict,
    pub baseline_total: u64,
    pub actual_total: u64,
    /// `(actual - baseline) / baseline * 100`. `0.0` when baseline is 0.
    pub delta_pct: f64,
    /// Per-fixture breakdown sorted by `delta_pct` descending so the
    /// worst offender is first (triage UX — User Story 2).
    pub per_fixture: Vec<FixtureDelta>,
    /// Fixtures present in the baseline but missing from the report,
    /// or vice versa. Article IV: surface coverage drift loud.
    pub coverage_drift: Vec<String>,
    /// Stage 1 advisory: when an all-zero baseline accepts a first
    /// non-zero report in Lenient mode, this is true.
    pub first_baseline_seed_recommended: bool,
}

impl serde::Serialize for RegressionVerdict {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Pass => s.serialize_str("pass"),
            Self::Fail => s.serialize_str("fail"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureDelta {
    pub name: String,
    pub baseline_micros: u64,
    pub actual_micros: u64,
    pub delta_pct: f64,
}

impl Baseline {
    /// Load a baseline snapshot from disk. Path typically
    /// `terrashift-evals/eval-baseline.json`.
    pub fn load(path: &Path) -> Result<Self, EvalError> {
        let bytes = std::fs::read(path).map_err(|e| EvalError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        serde_json::from_slice(&bytes).map_err(|e| EvalError::MappingPlanParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })
    }

    /// Helper for tests + bootstrap: build a baseline from a current
    /// report. Used to seed the file or to refresh it consciously.
    pub fn from_report(report: &SuiteReport, last_refreshed_iso: impl Into<String>) -> Self {
        let mut per_fixture = BTreeMap::new();
        for r in &report.results {
            per_fixture.insert(
                r.name.clone(),
                FixtureBaseline {
                    token_cost_micros: r.token_cost_micros,
                },
            );
        }
        Self {
            total_token_cost_micros: report.total_token_cost_micros,
            per_fixture,
            last_refreshed_iso: last_refreshed_iso.into(),
        }
    }
}

/// Compare a fresh report against the committed baseline. Returns a
/// `RegressionReport` with verdict + per-fixture breakdown + coverage
/// drift (Article IV).
pub fn compare_against_baseline(
    report: &SuiteReport,
    baseline: &Baseline,
    mode: BaselineMode,
) -> RegressionReport {
    let actual_total = report.total_token_cost_micros;
    let baseline_total = baseline.total_token_cost_micros;

    // Per-fixture deltas.
    let mut per_fixture: Vec<FixtureDelta> = Vec::with_capacity(report.results.len());
    for r in &report.results {
        let baseline_micros = baseline
            .per_fixture
            .get(&r.name)
            .map(|fb| fb.token_cost_micros)
            .unwrap_or(0);
        per_fixture.push(FixtureDelta {
            name: r.name.clone(),
            baseline_micros,
            actual_micros: r.token_cost_micros,
            delta_pct: pct_change(baseline_micros, r.token_cost_micros),
        });
    }
    // Sort by delta_pct descending — worst regression first (US2).
    per_fixture.sort_by(|a, b| {
        b.delta_pct
            .partial_cmp(&a.delta_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Coverage drift: fixtures in baseline but not in report (or
    // vice versa).
    let mut coverage_drift = Vec::new();
    let report_names: std::collections::BTreeSet<&String> =
        report.results.iter().map(|r| &r.name).collect();
    for k in baseline.per_fixture.keys() {
        if !report_names.contains(k) {
            coverage_drift.push(format!("baseline-only: {}", k));
        }
    }
    for r in &report.results {
        if !baseline.per_fixture.contains_key(&r.name) {
            coverage_drift.push(format!("report-only: {}", r.name));
        }
    }

    // Verdict.
    let delta_pct = pct_change(baseline_total, actual_total);
    let (verdict, seed_recommended) = decide_verdict(baseline_total, actual_total, mode);

    RegressionReport {
        verdict,
        baseline_total,
        actual_total,
        delta_pct,
        per_fixture,
        coverage_drift,
        first_baseline_seed_recommended: seed_recommended,
    }
}

/// Compute `(actual - baseline) / baseline * 100`. Returns 0.0 when
/// baseline is 0 (the "no LLM yet" Stage 1 ground state).
fn pct_change(baseline: u64, actual: u64) -> f64 {
    if baseline == 0 {
        return 0.0;
    }
    let baseline_f = baseline as f64;
    let actual_f = actual as f64;
    (actual_f - baseline_f) / baseline_f * 100.0
}

/// Decide the verdict + advisory flag.
fn decide_verdict(baseline: u64, actual: u64, mode: BaselineMode) -> (RegressionVerdict, bool) {
    // Both zero — no LLM yet — always Pass (Stage 1 invariant).
    if baseline == 0 && actual == 0 {
        return (RegressionVerdict::Pass, false);
    }
    // Baseline zero, actual non-zero — first-non-zero seed.
    if baseline == 0 && actual > 0 {
        return match mode {
            BaselineMode::Lenient => (RegressionVerdict::Pass, true),
            BaselineMode::Strict => (RegressionVerdict::Fail, false),
        };
    }
    // Both non-zero — apply the threshold.
    let pct = pct_change(baseline, actual);
    if pct > REGRESSION_THRESHOLD_PCT {
        (RegressionVerdict::Fail, false)
    } else {
        (RegressionVerdict::Pass, false)
    }
}

/// Convenience: full path of the canonical baseline file relative to
/// the workspace root. Tests + CI use this.
pub fn default_baseline_path(suite_root: &Path) -> PathBuf {
    suite_root.join("eval-baseline.json")
}
