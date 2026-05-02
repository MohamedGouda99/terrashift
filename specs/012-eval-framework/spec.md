# Spec — P-12: Eval framework (golden-file harness)

**Stage:** 1 | **P-NN:** P-12 | **Tier:** All
**TERRASHIFT_MAPPING.md:** §C Phase-2 (`libs/eval` is one of the seam crates).
**Constitution:** Article III (evals are source of truth for AI safety —
Validator, Mapper, Generator regressions caught before review),
Article XII rule 4 (>30% token cost regression blocks merge — gate plumbing
ships now; full activation in S7 with 10 goldens),
Article XIII rule 2 (cache-stability — non-monotonic trim breaks regression
detection; surfaces via determinism gate).
**Source pattern:** `refs/stakpak_arch.md` §32 (CI matrix);
`refs/stakpak/.github/workflows/ci.yml:38-43` (feature-gated test invocation
shape — verbatim mirror for S7 expansion);
`refs/stakpak/libs/ak/src/skills.rs:14-33` (golden-file `include_str!` +
substring assertion pattern, deviation noted: we use directory diffs not
single-file diffs).

## Goal

A deterministic, hermetic golden-file harness that runs Terrashift's
deterministic pipeline (Scanner → Generator in S3b; full pipeline once
Mapper/Validator/Executor land) against hand-curated source/expected pairs,
and reports pass/fail with diffs + token cost. Wires the Article XII
rule 4 regression gate so every PR gets a comparable cost signal.

## Stage 1 (S3b) scope

- `GoldenMigration` schema: `{ name, source_tf_dir, mapping_plan_json,
  expected_target_dir, manifest }`.
- `GoldenManifest` (per-fixture TOML): token cost ceiling (micros), articles
  asserted, optional source_provider/target_provider tags.
- `EvalRunner::run(&golden) -> EvalResult`: drives Generator over the
  fixture's pre-curated `MappingPlan` JSON, compares output against
  `expected/`, returns `EvalResult { passed, token_cost_micros, wall_clock_ms,
  diff }`.
- `run_suite(suite_root) -> SuiteReport`: discovers every subdirectory
  containing a `manifest.toml`, runs `EvalRunner::run` on each, aggregates.
- 3 hand-curated goldens at `terrashift-evals/` (workspace root, committed):
  - `001_aws_vpc_minimal/` — single VPC, exercises basic block emission.
  - `002_aws_subnet_with_reference/` — VPC + subnet referencing the VPC,
    exercises `AttributeValue::Reference` raw-HCL emit.
  - `003_aws_s3_bucket/` — single S3 bucket, exercises another resource family.
- Comparison strategy: **byte-equality** between generated and expected
  `.tf` files. Article VI guarantees Generator output is byte-stable;
  `insta`-style snapshot crate deferred until goldens grow past ~10 (S7).
- Tests: `EvalRunner::run` against each of the 3 goldens reports `passed: true`;
  intentionally-broken golden (mutated `expected/`) reports `passed: false`
  with a non-empty diff.

## Stage 1 (S3b) out of scope

- LLM token cost computation: `token_cost_micros = 0` placeholder for S3b.
  Real costs land in S4 when Mapper hits Groq.
- CI feature-gating per Stakpak's `ci.yml:38-43` (4-line shell-multiline
  pattern). Stage 1's 3 fast goldens run inside `cargo test --workspace`;
  feature-gated step arrives in S7 when goldens start exercising LLMs.
- Real `Mapper` integration — fixtures pre-curate a `mapping_plan.json`
  that emulates Mapper output. Replaced by real Mapper invocation in S4.
- Token-cost regression gate (Article XII rule 4 active enforcement):
  plumbing exists (`baseline.rs` placeholder will read median-of-N from a
  cached file), full activation S7 with 10 goldens.
- Structural-equivalence diff (round-trip via Scanner): nice-to-have,
  deferred until byte-equality proves brittle.

## Success criteria

`cargo test -p terrashift-eval` covers:

1. **Suite discovery:** `run_suite(terrashift-evals/)` finds exactly 3
   `manifest.toml` files and produces 3 `EvalResult`s.
2. **All 3 goldens pass:** with the Generator from P-08 (commit `07d26b3`),
   each fixture's actual output is byte-identical to its `expected/`.
3. **Diff on mismatch:** mutating one byte in an expected file produces
   `passed: false` and a non-empty `diff` field naming the differing file.
4. **Manifest parsing:** `GoldenManifest` deserializes the TOML form
   correctly (`token_cost_ceiling_micros`, `articles`, optional fields).
5. **Determinism cross-check:** running the suite twice produces
   byte-identical `SuiteReport`s aside from `wall_clock_ms`.
6. **Hermetic isolation:** the runner uses `tempfile::TempDir` per fixture
   so concurrent runs don't collide on `.terrashift/runs/`.
7. **Clippy clean:** workspace deny-lints (Article XIII rule 3) hold.

All 4 build gates green: fmt + clippy + check + test (workspace).
