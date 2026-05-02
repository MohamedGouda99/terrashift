# Cross-artifact consistency analysis — P-04

## Spec ↔ Plan ↔ Tasks ↔ Output

| Concern | Spec | Plan | Tasks | Output | Aligned? |
|---|---|---|---|---|---|
| Module structure | scanner/ submodule | 5 files | T3-T7 | 5 files written (mod, errors, inventory, parser, walker) | ✅ |
| EstateInventory location | `libs/engine/src/scanner/inventory.rs` (clarify Q1) | inventory.rs | T4 | Written there | ✅ |
| Attribute representation | String (clarify Q2) | String | T4 | `attributes: BTreeMap<String, String>` | ✅ |
| SourceSpan fidelity | `(file, line, col)` start (clarify Q3) | inventory.rs SourceSpan | T4 | Present, all three fields | ✅ |
| Failure mode | Hard-fail dynamic only (clarify Q4) | parser.rs check | T5 | Hard-fail dynamic; note count/for_each/provisioner/lifecycle | ✅ |
| Data sources | Parsed (clarify Q5) | parse_data_source | T5 | Implemented + tested | ✅ |
| Tests | Synthetic + fixture (clarify Q6) | T9 | T9 | 6 synthetic + 1 fixture-conditional, total 7 | ✅ |
| walkdir dep | Add to workspace + engine | T1, T2 | T1, T2 | Added in both Cargo.tomls | ✅ |
| Public API | `Scanner::scan(root)`, `Scanner::parse_file()` | mod.rs API | T7 | Both methods present | ✅ |
| Citation | terrashift_plan.md §5/§11 + Article I/IV | plan citation | (per file) | Every file has the doc-comment header | ✅ |

## Notes

**No Stakpak counterpart for Scanner:** Confirmed — Stakpak is a general DevOps
agent without HCL parsing. This is genuinely Terrashift-specific. Reference
discipline (Article II) doesn't apply because there's no Stakpak source to
mirror; the design follows terrashift_plan.md §5 directly.

**`approx_span` is a stage 1 heuristic:** hcl-rs 0.18 doesn't expose Span on
Block. We find the first `<kind> "` or `<kind> {` substring as a proxy.
Works for normal Terraform; fails when the same kind appears multiple times
in one file at the same indentation. Replace with proper span tracking when
hcl-rs 0.19+ exposes it (or on Stage 5 when we ship structured error UI).

**`string_slice` deny-lint hit on iteration 2:** Caught by clippy on the
first parser.rs version using `&content[..byte_offset]`. Replaced with
`content.get(..byte_offset).unwrap_or("")` which is char-safe (returns
`Option<&str>`, `None` if the byte offset would split a UTF-8 character).
Article XIII rule 3 enforcement worked exactly as designed — caught a real
class of bug at compile time.

## Drift detected

None. All clarify decisions reflected in the implementation. All 7 tests pass
on the post-fix build (verified via `cargo test`). Awaiting final clippy +
fmt re-verification.

## Verdict

**SAFE TO COMMIT** pending the re-verification pipeline (clippy + fmt after
the string_slice fix).
