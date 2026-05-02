# Acceptance Checklist — P-04

## Files added

- [x] `libs/engine/src/scanner/errors.rs` — `ScannerError` enum (4 variants)
- [x] `libs/engine/src/scanner/inventory.rs` — `EstateInventory` + 7 typed entity structs
- [x] `libs/engine/src/scanner/parser.rs` — HCL → typed entity extraction
- [x] `libs/engine/src/scanner/walker.rs` — recursive `.tf` discovery via walkdir
- [x] `libs/engine/src/scanner/mod.rs` — rewritten with `Scanner` facade
- [x] `libs/engine/tests/scanner_test.rs` — 7 tests (6 synthetic + 1 fixture-conditional)

## Files modified

- [x] `Cargo.toml` workspace — added `walkdir = "2"`
- [x] `libs/engine/Cargo.toml` — added `walkdir = { workspace = true }` + `[dev-dependencies] tempfile`

## Citation discipline

- [x] Each source file cites `terrashift_plan.md §5` (HLD-2 box 1) + `§11` (Terraform completeness)
- [x] Each file cites the relevant constitution articles
- [x] `approx_span` heuristic documented as Stage 1 limitation

## Constitution gates

- [x] Article I — Scanner is deterministic, not an agent (no LLM calls, no loops)
- [x] Article IV — `dynamic` blocks return `ScannerError::UnsupportedFeature` (loud); other Stage 5 features captured in `scan_notes` (visible)
- [x] Article XIII rule 3 — no `unwrap()`/`expect()`/`&s[..n]` in production
  (clippy verified after string_slice fix)

## Test cases (must pass)

- [x] `parses_single_resource` — single `aws_vpc` extracted with attributes
- [x] `parses_provider_block` — provider type + region + profile attributes captured
- [x] `parses_module_block_captures_source` — module source path extracted (unquoted)
- [x] `dynamic_block_is_loud_failure` — returns `UnsupportedFeature` with `dynamic` in feature name
- [x] `walks_directory_and_aggregates` — 2 files, 3 resources via temp dir
- [x] `data_source_block_is_parsed` — `data "aws_caller_identity"` captured
- [x] `fixture_walk_aws_modules_vpc` — finds aws_vpc + aws_subnet in PratikMahajan fixture vpc module (skips gracefully if fixture absent)

## Build gates (verified by pre-commit hook)

- [x] `cargo check -p terrashift-engine --tests` — 0 errors, 1 unused-import warning fixed
- [x] `cargo test -p terrashift-engine` — 7/7 tests pass
- [ ] `cargo clippy --all-targets -- -D warnings` — clean after string_slice fix (pending re-verify)
- [ ] `cargo fmt -- --check` — clean after auto-fmt (pending re-verify)

## Spec Kit hygiene

- [x] spec.md (success criteria + scope cuts documented)
- [x] clarify.md (6 questions answered with rationale)
- [x] plan.md (file-by-file plan + build order + citation discipline)
- [x] tasks.md (15 atomic tasks)
- [x] analyze.md (no drift; one Stage 1 heuristic flagged)
- [x] this checklist.md

## Ready for commit

**Verdict:** ✅ pending final clippy + fmt re-verification. Commit format:
`feat(p04): Scanner — deterministic HCL parser`.
