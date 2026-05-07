# Eval schema cache

Compressed provider schemas (`<provider>@<version>.json.zst`) referenced by
fixture `manifest.toml` `required_schemas` declarations. The eval framework
checks file presence here before running the Generator (RFC
schema-source-migration §5.1).

## Stage 1: placeholder content

The current files are **placeholders**, not real schemas. They satisfy the
existence check that `EvalRunner::run` performs but are not valid zstd-
compressed JSON. Real captures land when `cargo xtask capture-eval-schemas`
gets its non-dry-run path in S5 (see `xtask/src/capture_eval_schemas.rs`).

This intentional asymmetry is honest: Stage 1 evals are pure byte-stable
codegen (`scorer.rs` byte-equality), so the placeholders are sufficient
for the only consumer that exists today. S5 brings actual schema-driven
validation into evals; at that point the placeholders are replaced by
real captures and `cargo xtask verify-eval-schemas` becomes load-bearing
in CI.

## Files

| File | Used by |
|---|---|
| `aws@5.30.0.json.zst` | 001, 002, 003, 006, 007, 011, 015 (7 fixtures) |
| `azurerm@3.110.0.json.zst` | 004, 005, 008, 009, 010 (5 fixtures) |
| `google@5.40.2.json.zst` | 012 (1 fixture) |

## Versions

Pinned to match `libs/knowledge/seed/manifest.toml` so the bundled cache
and the eval cache exercise the same provider versions. When that file's
pins move, this cache moves with them — same PR.

## Adding a fixture that needs a new schema version

1. Declare the pin in the fixture's `manifest.toml`:
   ```toml
   required_schemas = [
     { provider = "aws", version = "5.99.0" },
   ]
   ```
2. Either add the placeholder file here (`touch aws@5.99.0.json.zst`) for
   Stage 1, or — once S5 lands — run `cargo xtask capture-eval-schemas`
   to populate it.
3. Run `cargo nextest run -p terrashift-eval` to confirm the fixture
   loads and runs.
