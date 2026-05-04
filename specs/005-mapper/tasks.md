---
description: "Task list for P-05 Mapper structural"
---

# Tasks: Mapper — single-LLM-call structured output

**Input**: `specs/005-mapper/{spec,plan}.md`
**Tests**: required (per FR-001 through FR-009)

## Phase 1: Setup

- [ ] **T001** Verify `libs/engine/Cargo.toml` already has
      `terrashift-ai`, `terrashift-knowledge`, `serde`,
      `serde_json`, `tokio`, `async-trait`, `thiserror`, `tracing`,
      `uuid`. Need to add: `sha2 = { workspace = true }`.

## Phase 2: Foundational

- [ ] **T002** [P] Write `libs/engine/src/mapper/errors.rs` —
      `MapperError` enum (every variant named per FR-005).
- [ ] **T003** [P] Write `libs/engine/src/mapper/context.rs` —
      `ContextReducer` trait + `PassthroughContextReducer` (Article
      XIII rule 1 seam; narrower signature than Stakpak's, doc'd).
- [ ] **T004** [P] Write `libs/engine/src/mapper/cache.rs` —
      `estate_cache_key(&EstateInventory) -> String` (Sha256 hex of
      `serde_json::to_vec`) + `MapperCache` HashMap wrapper.

**Checkpoint**: types ready; user-story phases can begin.

## Phase 3: User Story 1 — Mapper happy path (P1)

- [ ] **T005** [US1] Write `libs/engine/src/mapper/prompt.rs` —
      `SYSTEM_PROMPT: &str` const + `build_user_prompt(source_provider,
      target_provider, inventory, knowledge_hits) -> String`.
- [ ] **T006** [US1] Update `libs/engine/src/mapper/mod.rs` —
      `Mapper` struct + `async fn map(...)`. Body: empty-inventory
      short-circuit → cache lookup → build messages → reducer.reduce
      → llm.complete → parse JSON → cache insert → return.
- [ ] **T007** [US1] Tests: `happy_path_with_stub_client`,
      `empty_inventory_skips_llm_call`, `malformed_json_is_loud_error`.

## Phase 4: User Story 2 — Article XIII rule 1 enforcement (P1)

- [ ] **T008** [US2] Test stub `CountingContextReducer` in test file;
      `tests/mapper_test.rs::reducer_is_on_path` asserts count == 1.
- [ ] **T009** [US2] Test stub `CountingLlmClient` (wraps
      `StubClient`); `cache_hit_avoids_llm_call` asserts count == 1
      after two `map()` calls with same inventory.

## Phase 5: User Story 3 — Knowledge hits in prompt (P2)

- [ ] **T010** [US3] Test `knowledge_hits_inform_prompt` —
      `RecordingLlmClient` captures the prompt; assertion checks
      knowledge resource type strings appear.

## Phase 6: Wiring + Build Gates

- [ ] **T011** Update `libs/engine/src/mapper/mod.rs` re-exports.
- [ ] **T012** `cargo check -p terrashift-engine --tests`.
- [ ] **T013** `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **T014** `cargo fmt -- --check`.
- [ ] **T015** `cargo test --workspace`.

## Phase 7: Review + Commit

- [ ] **T016** Self-review (reviewer agents quota-limited; checklist
      walked manually).
- [ ] **T017** `eval-runner` agent — verify the 5 goldens still pass
      (Mapper changes the engine's deps but doesn't modify the
      Generator path goldens travel through).
- [ ] **T018** Write `analyze.md` + `checklist.md`.
- [ ] **T019** Update `SESSION_PLAN.md` row 4b — note Mapper
      structural shipped; full S4-close awaits `GROQ_API_KEY`.
- [ ] **T020** Commit `feat(p-05): Mapper — single-LLM-call structured
      output (S4b structural)`.
