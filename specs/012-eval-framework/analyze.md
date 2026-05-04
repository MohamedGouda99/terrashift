# Analysis — P-12 (post-implementation)

## Decisions taken at implementation time (not in spec)

### A. `AttributeValue` serde representation switched to externally tagged

Spec said: clarify Q4 chose `mapping_plan.json` per golden. Implementation
hit a serde derive surprise: my P-08 declaration of `AttributeValue` used
`#[serde(tag = "kind")]` (internally tagged), but serde rejects internal
tagging on tuple-newtype variants whose inner type is a primitive (not a
struct/map). `String(String)`, `Reference(String)`, `Bool(bool)`,
`Number(f64)` would all runtime-error on deserialize.

Fix: dropped `tag = "kind"`, kept `rename_all = "snake_case"`. Now
externally tagged: `{ "string": "10.0.0.0/16" }`, `{ "reference": "aws_vpc.main.id" }`.
JSON shape is more verbose by one wrapping level but trivially round-trips.
This is a P-08 type change shipped in the P-12 commit because P-12
fixtures were the first place we tried serde on `AttributeValue`.

Test `attribute_value_json_round_trips` locks the new shape.

### B. Bootstrap helper as a `BOOTSTRAP_GOLDENS=1` test instead of a binary

Spec didn't address how `expected/` gets seeded. Two paths considered:
1. Hand-author the expected `.tf` (tedious; risk: small whitespace
   mismatches with hcl-rs output).
2. Run Generator on each fixture, capture actual, commit as expected
   (correct-by-construction; risk: bootstrap procedure has to be
   discoverable).

Picked #2, implemented as a `bootstrap_goldens` test gated on the
`BOOTSTRAP_GOLDENS=1` env var. Keeps the procedure colocated with the
test file, no extra crate target. README documents the one-liner.

Trade-off: `bootstrap_goldens` always shows in the test list as "passed"
even when not invoked (it returns early). Acceptable because: (a) it's
discoverable, (b) it's the canonical way to add new fixtures.

### C. EvalError variants boxed for `result_large_err`

`toml::de::Error` and `serde_json::Error` are ~120 bytes each. Without
boxing, `EvalError` triggers clippy `result_large_err` because the
`Err`-variant becomes >128 bytes. Fix: `Box<toml::de::Error>` and
`Box<serde_json::Error>` in `EvalError`. Stakpak's `libs/audit` doesn't
hit this lint because their error types use less context; ours pull in
TOML and JSON parsers which carry richer error context.

This is a real improvement over an uncovered Stakpak gap — the lint
exists for a reason (large `Result` types pessimize hot paths).

### D. `EvalRunner::run_suite` is fail-fast on load errors

If any fixture's `manifest.toml`, `mapping_plan.json`, or `expected/`
is missing, `discover_suite` returns Err early — Article IV ("loud at
load time, not silently skip"). An alternative was to collect partial
results with per-fixture `EvalResult { passed: false, diff: Some("load
error") }`. Rejected because conflating "fixture broken on disk" with
"fixture failed comparison" makes the SuiteReport harder to interpret.

If S7 needs partial-success behaviour (e.g., one broken fixture
shouldn't block the rest), revisit then.

## Risks resolved

| Risk (from plan.md) | Outcome |
|---|---|
| Generator output drift breaks all 3 goldens | Drift is *the* signal P-12 catches. Test `all_three_goldens_pass` is deliberately strict — any change to template emit breaks it loudly. |
| Manifest TOML format misses fields S4 needs | `#[serde(default)]` on optional fields ensures backward-compat additions. Verified by `manifest_toml_parses_correctly`. |
| `similar` diff output too noisy on multi-file mismatches | Diff scoped per-file with clear `~ <filename>:` headers. CI logs stay readable. |
| Goldens duplicate effort with P-08 tests | P-08 tests Generator mechanics in isolation; goldens validate paired contracts (source+plan→expected) — orthogonal invariant. |

## Stakpak / Claude Code reference findings

Per the `reference-explorer` agent (`Pattern extraction §A-D`):

1. **Stakpak ships no eval/golden infra** beyond a single `include_str!`
   byte-equality test in `libs/ak/src/skills.rs:14-33`. We imported the
   discipline (byte-equality on a known-stable format) but built our own
   directory-level harness.
2. **Claude Code refs/ has zero test scaffolding** — no `.github/workflows/`,
   no `package.json`, no `*.test.ts`. Nothing to borrow.
3. **CI shape adopted from Stakpak** (`refs/stakpak/.github/workflows/ci.yml:38-43`)
   — single-job, multi-step, feature-gated. Stage 1 runs goldens inline;
   feature-gating activates in S7 when goldens exercise LLMs.

## What's NOT here (Stage 1 deferrals)

- **`insta` snapshot crate** (clarify Q2) — defer to S7 expansion.
- **CI feature-gating** (clarify Q6) — defer to S7.
- **Real Mapper integration** — fixtures pre-curate `mapping_plan.json`;
  S4 wires real Mapper output through the same JSON contract.
- **Token-cost regression gate active enforcement** (Article XII rule 4)
  — plumbing is shipped (`token_cost_micros` in `EvalResult`,
  `token_cost_ceiling_micros` in `GoldenManifest`) but no CI gate yet;
  S7 with 10 goldens.
- **Audit-log cross-check** — eval framework doesn't yet verify each run
  produces the expected `AuditPayload` chain; that's a P-15 follow-up.
- **Round-trip via Scanner** — for now we trust Generator's P-08
  round-trip test; eval framework just byte-compares output.

## Spec criteria coverage

All 7 numbered criteria from spec.md `## Success criteria` have a test:

| Criterion | Test |
|---|---|
| #1 suite discovery | `discover_suite_finds_three_goldens` |
| #2 all 3 goldens pass | `all_three_goldens_pass` |
| #3 diff on mismatch | `mutated_expected_produces_diff` |
| #4 manifest parsing | `manifest_toml_parses_correctly` |
| #5 determinism | `suite_is_deterministic_across_runs` |
| #6 hermetic isolation | covered by `EvalRunner::run` using `TempDir` per call (every test exercises this) |
| #7 clippy clean | enforced by `cargo clippy -- -D warnings` |

Plus one bonus: `attribute_value_json_round_trips` locks the
serde-shape change from §A above.

## Cross-file consistency

- `libs/eval/src/lib.rs` re-exports the 4 modules.
- `libs/engine/src/mapper/mod.rs` `AttributeValue` change is backward-
  compatible at the Rust level (same variants, same names) but breaks
  any pre-existing JSON file with the old internally-tagged shape.
  No such files exist in the repo (P-08 only used the type via Rust
  constructors in tests), so the change is safe.
- `Cargo.toml` workspace adds `similar = "2"`; `libs/eval/Cargo.toml`
  adds `similar`, `tempfile`, `toml`, `uuid`.
