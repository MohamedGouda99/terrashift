# Feature Specification: R6 — Article XII rule 4 baseline + CI regression gate

**Feature Branch**: `r06-eval-baseline-gate`
**Created**: 2026-05-02
**Status**: Draft (Stage 1 baseline = 0; gate in Lenient mode; flips to Strict in S4-close once LLM costs land)
**Input**: Stage 1 P-16 stage-gate **R6** remediation ticket (this commit: <hash> — supersedes the placeholder TODO in `.github/workflows/ci.yml`).
**Reference**: `terrashift_plan.md` §6.X token-economy framing,
`Terrashift_Plan.docx` §6.4 (regression-gate motivation), Stage 1 P-16
criterion #4 (token cost <\$15 for sample migration; transitions from
NOT MEASURABLE to ACTIVELY ENFORCED).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — CI catches a >30% token-cost regression on PR (Priority: P1) 🎯 MVP

A future PR (S4-close or beyond) introduces a prompt change that
inflates Mapper token cost by 35%. The eval CI job runs the 10-golden
suite, compares the new `SuiteReport.total_token_cost_micros` to the
committed `eval-baseline.json`, sees a regression > 30%, and **fails the
build** with a structured message naming the offending fixtures + the
percentage delta. The PR cannot merge until the baseline is consciously
updated (a `chore: refresh eval baseline` commit) or the regression is
fixed.

**Why this priority**: Article XII rule 4 is the load-bearing token-
discipline mechanism. Without it, prompt drift silently makes every
customer's bill bigger. Plumbing that ships now (with a 0 baseline) is
*structural readiness* — when the first non-zero baseline lands in
S4-close, the gate is already armed.

**Independent Test**: Construct a synthetic `SuiteReport` whose
`total_token_cost_micros` is 1.5× a fixture baseline, call
`compare_against_baseline(report, baseline, BaselineMode::Strict)`,
assert the result is `RegressionVerdict::Fail` with the offending
fixture named in the message.

**Acceptance Scenarios**:
1. **Given** a baseline `total = 1_000_000` micros and a report `total = 1_400_000` micros (+40%), **When** `compare_against_baseline(report, baseline, Strict)`, **Then** verdict is `Fail` with `delta_pct ≈ 40` and the message names which fixture(s) regressed.
2. **Given** a baseline `total = 1_000_000` and a report `total = 1_200_000` (+20%, under the 30% threshold), **When** compared in Strict, **Then** verdict is `Pass` with a `regression_pct: 20` field for diagnostic.
3. **Given** a Stage 1 baseline `total = 0` and a report `total = 0`, **When** compared in any mode, **Then** verdict is `Pass` (the all-zero baseline is the no-LLM Stage 1 invariant).
4. **Given** a Stage 1 baseline `total = 0` and a report `total = 5_000_000` (first non-zero, Lenient mode — S4-close), **When** compared, **Then** verdict is `Pass` with a `first_baseline_seed_recommended: true` advisory.
5. **Given** a Stage 1 baseline `total = 0` and a report `total = 5_000_000` in Strict mode, **When** compared, **Then** verdict is `Fail` (Strict refuses to accept a first-non-zero baseline; operator must consciously refresh the baseline file).

### User Story 2 — Per-fixture diff in the regression report (Priority: P2)

When a regression fires, the operator needs to know **which fixtures
regressed**, not just that the total moved. The report should list each
fixture's `(baseline_micros, actual_micros, delta_pct)` so the operator
can localise the prompt change that caused the spike.

**Why this priority**: triage UX. Without per-fixture detail, "your eval
bill went up 31%" tells the operator approximately nothing.

**Independent Test**: 3 fixtures, baseline `[100, 200, 300]`, report
`[100, 200, 600]` (third fixture +100%); assert the regression report's
`per_fixture` list flags the third fixture with `delta_pct == 100`.

**Acceptance Scenarios**:
1. **Given** 3 fixtures with mixed deltas, **When** comparison runs, **Then** `RegressionReport.per_fixture` contains 3 entries naming each fixture + its baseline + actual + delta_pct, sorted by delta_pct descending.

### User Story 3 — Stage 1 baseline file commits cleanly with all-zero values (Priority: P1)

Today, no LLM calls fire — every fixture's `token_cost_micros == 0`.
The committed `eval-baseline.json` reflects this honestly.

**Why this priority**: the file's existence + correctness IS the
shippable Stage 1 deliverable. S4-close's job is to refresh this file
with real LLM cost numbers; until then, the file documents
"this is the no-LLM ground state."

**Independent Test**: `Baseline::load(path)` parses
`terrashift-evals/eval-baseline.json` without error and the loaded
`per_fixture` map contains every one of the 10 goldens with
`token_cost_micros: 0`.

**Acceptance Scenarios**:
1. **Given** `terrashift-evals/eval-baseline.json` shipped with this PR, **When** `Baseline::load(path)`, **Then** the loaded baseline has `per_fixture.len() == 10` AND `total_token_cost_micros == 0`.

### Edge Cases

- **Missing `eval-baseline.json`** — `Baseline::load` returns
  `BaselineError::NotFound` with the path. CI should fail with a clear
  message ("regenerate via `cargo run --example refresh-baseline` once
  S4-close lands"). Article IV.
- **Baseline has fixtures the report doesn't (or vice versa)** —
  surface as a `RegressionReport.coverage_drift` warning. Doesn't fail
  the build by itself, but the operator should refresh the baseline.
- **Division by zero** — when computing `delta_pct = (actual - baseline) / baseline`, baseline = 0 short-circuits per scenario #3/#4/#5 above.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Add `Baseline { total_token_cost_micros, per_fixture: BTreeMap<String, FixtureBaseline> }` struct with serde Deserialize from JSON.
- **FR-002**: Add `compare_against_baseline(report, baseline, mode) -> RegressionReport` with `BaselineMode { Lenient, Strict }`.
- **FR-003**: `RegressionVerdict { Pass, Fail }` derived from total delta_pct vs the 30% threshold; `Pass` when `mode == Lenient && baseline.total == 0 && report.total > 0` (first non-zero seed allowed).
- **FR-004**: `RegressionReport.per_fixture` is sorted by `delta_pct` descending so the worst offender is first.
- **FR-005**: `Baseline::load(path)` reads the JSON file from disk; failure modes named (NotFound, InvalidJson, IoError).
- **FR-006**: A committed `terrashift-evals/eval-baseline.json` file with all 10 goldens at `token_cost_micros: 0`.
- **FR-007**: `.github/workflows/ci.yml` `eval` job runs the suite, loads the baseline, and exits non-zero on `Fail` verdict.
- **FR-008**: Article XIII rule 3 — production paths clippy-clean.

### Key Entities

- **`Baseline`** — committed JSON snapshot of expected per-fixture token costs.
- **`FixtureBaseline`** — `{ token_cost_micros: u64 }` per fixture (extensible: future fields like `wall_clock_ms_p95`).
- **`BaselineMode`** — `Lenient | Strict` enum.
- **`RegressionReport`** — `{ verdict, baseline_total, actual_total, delta_pct, per_fixture, coverage_drift }`.
- **`RegressionVerdict`** — `Pass | Fail`.
- **`BaselineError`** — `NotFound { path } | InvalidJson { source } | Io { path, source }`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test -p terrashift-eval --test baseline_test` runs ≥ 6 offline tests covering every acceptance scenario in US1-US3.
- **SC-002**: Committed `terrashift-evals/eval-baseline.json` parses via `Baseline::load` and contains all 10 goldens with 0 cost.
- **SC-003**: CI step shipped (visible diff in `.github/workflows/ci.yml`); a synthetic regression test verifies the gate fails when expected.
- **SC-004**: `30%` threshold is a named constant `REGRESSION_THRESHOLD_PCT` with a docstring linking Article XII rule 4.
- **SC-005**: Production paths in `libs/eval/src/baseline.rs` are clippy `unwrap_used` / `expect_used` / `string_slice` clean.

## Assumptions

- **`SuiteReport` already carries per-fixture costs** (P-12 commit `311b12c` — `EvalResult.token_cost_micros`).
- **The baseline file lives in `terrashift-evals/`** alongside fixtures (so `git mv terrashift-evals/ private-repo/` carries baseline along; per `specs/012-eval-framework/clarify.md` Q1).
- **JSON format chosen over TOML** for the baseline — TOML is for human-edited config, JSON is for machine-emitted data (per `specs/012-eval-framework/clarify.md` Q3-Q4 boundary).

## Appendix A — Why this is R6, not a new P-NN

R6 is a **remediation ticket** from the Stage 1 P-16 stage-gate review
(prior commit's report). P-NNs are the canonical pipeline-component
prompts in `terrashift_prompts.md`; R-NNs are stage-gate remediation
tickets that don't introduce new components but close gaps in
existing infrastructure. R6 closes Article XII rule 4's
"actively-enforced" status pending the LLM-cost data that S4-close
will provide.

The spec uses the official `.specify/templates/spec-template.md`
format because the discipline applies to all substantive code
changes, not just P-NNs.
