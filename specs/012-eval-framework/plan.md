# Plan — P-12

## Files

| File | Purpose | Lines (est) |
|---|---|---|
| `libs/eval/src/lib.rs` | Re-exports + crate doc | ~25 |
| `libs/eval/src/errors.rs` | `EvalError` enum (with `thiserror`) | ~50 |
| `libs/eval/src/golden.rs` | `GoldenMigration`, `GoldenManifest`, `load_golden`, `discover_suite` | ~150 |
| `libs/eval/src/runner.rs` | `EvalRunner`, `run_eval`, `run_suite`, `EvalResult`, `SuiteReport` | ~150 |
| `libs/eval/src/scorer.rs` | `compare_directories` byte-equality + `similar` unified diff | ~120 |
| `libs/eval/tests/eval_runner_test.rs` | Integration tests: 3 goldens pass, mutation produces diff, manifest parse, determinism | ~200 |
| `terrashift-evals/001_aws_vpc_minimal/` | Golden #1 (4 files: source.tf, mapping_plan.json, manifest.toml, expected/aws_vpc.tf) | ~40 |
| `terrashift-evals/002_aws_subnet_with_reference/` | Golden #2 (5 files: source, plan, manifest, expected/aws_vpc.tf, expected/aws_subnet.tf) | ~60 |
| `terrashift-evals/003_aws_s3_bucket/` | Golden #3 (4 files) | ~35 |
| `terrashift-evals/README.md` | Convention notes + "how to add a golden" | ~50 |

**Total est:** ~880 lines (production ~495 + tests ~200 + goldens+docs
~185). Within Article VII review scope.

## Build order

1. `errors.rs` (no deps)
2. `golden.rs` (deps: `serde`, `toml`, `terrashift-engine::mapper::MappingPlan`)
3. `scorer.rs` (deps: `similar`, `errors`)
4. `runner.rs` (deps: `golden`, `scorer`, `terrashift-engine::generator::Generator`)
5. `lib.rs` re-exports
6. Hand-curate 3 goldens under `terrashift-evals/`
7. Tests
8. `cargo check` + `clippy` + `fmt` + `test` workspace
9. `code-reviewer` agent → resolve findings
10. `constitution-checker` agent → resolve findings
11. Commit + SESSION_PLAN.md row 3b → ✅

## Cargo dep updates

`libs/eval/Cargo.toml` already has `serde`, `serde_json`, `thiserror`,
`anyhow`, `tracing`, `tokio`, `chrono` + the workspace internal crates
(`terrashift-shared`, `terrashift-engine`, `terrashift-audit`). Additions:

- `toml = { workspace = true }` — for `GoldenManifest` deserialization
- `similar = "2"` — unified-diff generator (small dep, one purpose)
- `uuid = { workspace = true }` — for fresh `run_id`s during eval runs
- `tempfile = "3"` (dev-dep) — hermetic test isolation

`similar` is the only new external dep; all others are workspace-shared.

## Golden migration on-disk shape

```
terrashift-evals/
└── 001_aws_vpc_minimal/
    ├── manifest.toml         # token_cost_ceiling_micros, articles
    ├── source.tf             # source-cloud HCL (e.g., google_compute_network)
    ├── mapping_plan.json     # pre-curated MappingPlan; replaces real Mapper until S4
    └── expected/
        └── aws_vpc.tf        # the .tf the Generator should emit byte-for-byte
```

`manifest.toml` shape:

```toml
name = "001_aws_vpc_minimal"
description = "Single VPC; exercises basic block emission"
source_provider = "google"
target_provider = "aws"
token_cost_ceiling_micros = 0   # Stage 1 placeholder; S4 sets real ceilings
articles = [3, 6, 9]            # which constitution articles this golden validates
```

## Public API shape

```rust
pub struct EvalRunner { generator: Generator }
impl EvalRunner {
    pub fn new() -> Self;
    pub fn run(&self, golden: &GoldenMigration) -> Result<EvalResult, EvalError>;
    pub fn run_suite(&self, suite_root: &Path) -> Result<SuiteReport, EvalError>;
}

pub struct EvalResult {
    pub name: String,
    pub passed: bool,
    pub token_cost_micros: u64,    // Stage 1: 0
    pub wall_clock_ms: u32,
    pub diff: Option<String>,       // None when passed
}

pub struct SuiteReport {
    pub results: Vec<EvalResult>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub total_token_cost_micros: u64,
}
```

## Citation pattern (every file head)

```rust
//! Pattern: stakpak_arch.md §32 (CI matrix; eval suite is a gated CI check).
//! Source: refs/stakpak/.github/workflows/ci.yml:38-43 (feature-gated tests
//!         shape — adopted for S7+ when goldens grow past ~10; Stage 1 runs
//!         inline via cargo test --workspace).
//! Constitution: Article III (evals are SoT for AI safety),
//!               XII rule 4 (regression gate plumbing), XIII rule 2.
```

## Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| Generator output drift breaks all 3 goldens | Low | P-08 determinism regression test fires first; goldens are downstream |
| Manifest TOML format misses fields S4 needs | Medium | `#[serde(default)]` for new fields keeps backward compat |
| `similar` diff output too noisy on multi-file mismatches | Low | Limit diff to first 50 lines; full diff in result for CI inspection |
| Goldens duplicate effort with P-08 tests | Low | P-08 tests verify Generator's mechanics; goldens verify *paired contracts* (source+plan→expected) which is a different invariant |
