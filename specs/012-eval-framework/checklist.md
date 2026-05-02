# Checklist — P-12

## Spec Kit gates

- [x] `spec.md` — goal, scope, out-of-scope, success criteria
- [x] `clarify.md` — 10 user-on-behalf decisions documented
- [x] `plan.md` — file table, build order, deps, risk register
- [x] `tasks.md` — T1..T21 atomic task list
- [x] `analyze.md` — post-impl decisions + risks + deviations
- [x] `checklist.md` — this file

## Build gates

- [x] `cargo check -p terrashift-eval --all-targets` — clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — clean
- [x] `cargo fmt --check` — clean
- [x] `cargo test --workspace` — all green; 0 failures

## Spec success criteria coverage

- [x] #1 suite discovery — `discover_suite_finds_three_goldens`
- [x] #2 all 3 goldens pass — `all_three_goldens_pass`
- [x] #3 diff on mismatch — `mutated_expected_produces_diff`
- [x] #4 manifest parsing — `manifest_toml_parses_correctly`
- [x] #5 determinism cross-check — `suite_is_deterministic_across_runs`
- [x] #6 hermetic isolation — `EvalRunner::run` uses `TempDir` per call
- [x] #7 clippy clean — enforced by `cargo clippy -- -D warnings`
- [x] **Bonus: serde round-trip** — `attribute_value_json_round_trips`

## Constitution coverage

- [x] **Article III** — eval framework IS the source of truth for
      Generator/Mapper/Validator regressions. 3 goldens validate the
      Stage 1 deterministic happy paths.
- [x] **Article IV** — every load failure in `EvalError` is named;
      `discover_suite` fails loudly on first broken fixture (no silent
      skips per analyze.md §D).
- [x] **Article VI** — byte-stable Generator output (proved by P-08
      `determinism_byte_identical_across_runs`) makes byte-equality
      comparison meaningful; `suite_is_deterministic_across_runs`
      double-confirms at the eval layer.
- [x] **Article XII rule 1** — `token_cost_ceiling_micros` field shipped
      in `GoldenManifest`; populated with 0 in Stage 1; S4 sets real
      ceilings.
- [x] **Article XII rule 4** — regression gate plumbing in place
      (`token_cost_micros` in `EvalResult`, `total_token_cost_micros` in
      `SuiteReport`); active enforcement S7 with 10 goldens.
- [x] **Article XIII rule 2** — non-monotonic trim breaks regression
      detection. Stage 1 goldens have 0 token cost (no Mapper) so this
      doesn't bite yet, but the discipline is wired (deterministic
      pipeline + byte-stable output = monotonic-by-construction).
- [x] **Article XIII rule 3** — no `unwrap`/`expect`/string-slice in
      production. Two `unwrap_or` uses (`filename_string` fallback,
      `suite_root` fallback) are `Option::unwrap_or` deliberate defaults.

## Stakpak / Claude Code reference fidelity

- [x] CI matrix shape per `refs/stakpak/.github/workflows/ci.yml:38-43`
      cited in `lib.rs` — feature-gating adopted in S7, not S3b.
- [x] Stakpak's eval/golden gap (no fixtures beyond a single `include_str!`
      test in `libs/ak/src/skills.rs`) is documented; we built our own
      directory-level harness.
- [x] Claude Code refs ship no test scaffolding — nothing borrowed
      (per analyze.md "Stakpak / Claude Code reference findings").

## Files committed

- [x] `specs/012-eval-framework/{spec,clarify,plan,tasks,analyze,checklist}.md` (6 files)
- [x] `Cargo.toml` (added `similar = "2"` to workspace deps)
- [x] `libs/eval/Cargo.toml` (added `similar`, `toml`, `uuid`, `tempfile` deps)
- [x] `libs/eval/src/errors.rs` (NEW — `EvalError` with boxed parse errors)
- [x] `libs/eval/src/golden.rs` (NEW — `GoldenMigration`, `GoldenManifest`, `load_golden`, `discover_suite`)
- [x] `libs/eval/src/scorer.rs` (NEW — directory comparison + unified diff)
- [x] `libs/eval/src/runner.rs` (NEW — `EvalRunner`, `EvalResult`, `SuiteReport`)
- [x] `libs/eval/src/lib.rs` (rewrote stub with re-exports)
- [x] `libs/eval/tests/eval_runner_test.rs` (NEW — 7 tests + bootstrap helper)
- [x] `libs/engine/src/mapper/mod.rs` (modified — `AttributeValue` switched to externally-tagged serde shape)
- [x] `terrashift-evals/README.md` (convention notes)
- [x] `terrashift-evals/{001,002,003}_*/` (3 hand-curated goldens, manifests + sources + plans + bootstrapped expected/)

## SESSION_PLAN ledger

- [x] Row 3b status flipped to ✅ with this commit's hash

## Reviewer pass

- [ ] `code-reviewer` subagent
- [ ] `constitution-checker` subagent
- [ ] Findings resolved
- [ ] Commit `feat(p-12): Eval framework — golden-file harness + 3 fixtures`
- [ ] `/stage-gate` to close S3b
