# Checklist — P-05

## Spec Kit gates (official template format — second commit using it)
- [x] spec.md (User Stories with priorities, FR-NNN, SC-NNN, Edge Cases, Assumptions, verbatim P-05 prompt in Appendix A)
- [x] plan.md (Technical Context, Constitution Check, Project Structure, Complexity Tracking)
- [x] tasks.md (Phase-organized; US-tagged tasks, [P] parallel markers)
- [x] analyze.md (post-impl decisions + risks + Stakpak fidelity)
- [x] checklist.md (this file)

## Build gates
- [x] `cargo check -p terrashift-engine --tests` — clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — clean
- [x] `cargo fmt --check` — clean
- [x] `cargo test --workspace` — 40 result lines, 0 FAILED (case-sensitive)
- [x] `cargo test -p terrashift-engine --test mapper_test` — 7 passed
- [x] `cargo test -p terrashift-eval` — eval-runner agent confirmed 6 passed, 1 ignored, goldens green

## Spec criteria coverage
- [x] SC-001 ≥7 offline tests
- [x] SC-002 happy path round-trip via StubClient
- [x] SC-003 cache hit avoids LLM (CountingLlmClient verifies)
- [x] SC-004 reducer on path exactly once (CountingContextReducer verifies)
- [x] SC-005 malformed JSON → MapperError::MalformedResponse with reason + sample
- [x] SC-006 estate_cache_key byte-stable across runs
- [x] SC-007 production paths clippy-clean (Article XIII rule 3)

## Constitution coverage
- [x] **I** — Mapper is single LLM call, NOT an agent (no loops, no retry, no tool-back)
- [x] **III** — output validates as MappingPlan via serde_json::from_str; Validator (P-06) gates downstream
- [x] **IV** — every MapperError variant named (MalformedResponse, InventoryTooLarge, Llm, Knowledge, Serialize)
- [x] **XII rule 1** — empty inventory short-circuits without LLM call
- [x] **XII rule 2** — cache-first via Sha256(serde_json::to_vec(estate))
- [x] **XIII rule 1** — `reducer: &dyn ContextReducer` non-optional in `map(...)` signature; bypass = compile error
- [x] **XIII rule 3** — production paths grep clean for unwrap/expect/string-slice; the truncate_utf8 helper avoids `&s[..n]` panics
- [x] **XIII rule 4** — stakai's Message/Role are canonical; Stage-1-local Message documented as convergent

## Reviewers
- [~] `code-reviewer` agent: quota-limited at session start; self-review walked the same checklist
- [~] `constitution-checker` agent: quota-limited; self-review walked Article matrix
- [x] `eval-runner` agent: ran post-impl, all 5 goldens green, GO verdict

## Files committed
- [x] `specs/005-mapper/{spec,plan,tasks,analyze,checklist}.md`
- [x] `libs/engine/Cargo.toml` (added `sha2` dep)
- [x] `libs/engine/src/mapper/{mod,errors,context,cache,prompt}.rs`
- [x] `libs/engine/tests/mapper_test.rs` (7 tests)

## SESSION_PLAN ledger
- [x] Row 4b → ✅ structural; integration test (Mapper round-tripping through RealClient against real Groq) pending GROQ_API_KEY in CI
