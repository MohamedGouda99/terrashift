---
description: "Task list for P-03 LLM client + BYOK resolver"
---

# Tasks: LLM client + 5-layer BYOK model resolution

**Input**: `specs/003-llm-byok-resolution/{spec,plan}.md`
**Tests**: required (per FR-001 through FR-009)
**Organization**: phased; Phase 1 setup, Phase 2 foundational types,
Phase 3 per-User-Story implementation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1 = profile resolution, US2 = stub
  client, US3 = real Groq integration)

## Path Conventions

Single workspace crate `libs/ai/`. All paths relative to repository root.

---

## Phase 1: Setup (Shared Infrastructure)

- [ ] **T001** Verify `libs/ai/Cargo.toml` deps include `stakai`,
      `tokio`, `serde`, `toml`, `async-trait`, `thiserror`, `tracing`.
      No additions needed (all present).
- [ ] **T002** Create `libs/ai/tests/` directory.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Types every user story depends on.

- [ ] **T003** [P] Write `libs/ai/src/tier.rs` — `Tier` enum
      (`Eco | Smart`), `Display` + `serde::Deserialize` derived.
- [ ] **T004** [P] Write `libs/ai/src/errors.rs` — `AiError` enum
      (`UnknownProvider`, `UnsupportedProviderType`, `MissingApiKey`,
      `EmptyPrompt`, `Stakai`, `TomlParse`, `IoError`).
- [ ] **T005** [P] Write `libs/ai/src/metadata.rs` —
      `CompletionMetadata` struct with token counts, provider,
      model_id, endpoint, latency_ms.

**Checkpoint**: Phase 2 complete; user story phases can begin.

---

## Phase 3: User Story 1 — Profile resolution (Priority: P1) 🎯 MVP

**Goal**: Operator's TOML profile parses cleanly; `Resolver` returns
the right `(provider_key, model_id, endpoint, api_key_env)` tuple per
the 5-layer chain.

**Independent Test**: `cargo test -p terrashift-ai resolver` —
fixture TOML + 5 priority-chain tests, all offline.

- [ ] **T006** [US1] Write `libs/ai/src/profile.rs` — `Profile`,
      `ProviderConfig`, `Tiers` serde structs; `Profile::load_toml(s)`
      helper.
- [ ] **T007** [US1] Write `libs/ai/src/resolver.rs` — `Resolver`
      pure-function struct; `resolve_for_tier(profile, cli_override,
      call_override, tier)` returning `ResolvedModel`.
- [ ] **T008** [US1] Tests in `tests/llm_client_test.rs`:
      `profile_round_trips_toml`, `resolver_picks_tier_from_profile`,
      `cli_override_wins_over_profile`,
      `per_call_override_wins_over_cli`,
      `unknown_provider_is_loud_error`.

**Checkpoint**: US1 deliverable — operator can author a profile and
predict exactly which model + endpoint will be used.

---

## Phase 4: User Story 2 — Hermetic StubClient (Priority: P1)

**Goal**: Tests / Mapper integration runs offline against
`StubClient` with canned responses.

**Independent Test**: `cargo test -p terrashift-ai stub` — 2 tests,
no network.

- [ ] **T009** [US2] Write `libs/ai/src/client.rs` — `LlmClient` trait
      (`async fn complete(tier, prompt) -> Result<(String,
      CompletionMetadata), AiError>`); `StubClient` impl with
      `with_response(prompt_substring, response)` builder + default
      response fallback.
- [ ] **T010** [US2] Tests: `stub_returns_canned_response`,
      `stub_default_fallback`.

**Checkpoint**: US2 deliverable — Mapper tests in S4 can use
`StubClient` from day 1.

---

## Phase 5: User Story 3 — Real Groq integration (Priority: P2)

**Goal**: When `GROQ_API_KEY` is set, real network round-trip works.

**Independent Test**: `BOOTSTRAP_GROQ=1 cargo test -p terrashift-ai
real_groq_completes -- --ignored` against a live key.

- [ ] **T011** [US3] Write `libs/ai/src/real.rs` — `RealClient`
      wrapping `stakai::Inference`. Stage 1: only `openai-compatible`
      provider type. Translate `Tier` + prompt to
      `stakai::GenerateRequest`.
- [ ] **T012** [US3] Add `#[ignore]`'d test
      `real_groq_completes` — only fires under `--ignored` AND
      env-var-gated to avoid surprise in unrelated `--ignored` runs.

**Checkpoint**: US3 deliverable — S4-close path is unblocked once
the operator provides `GROQ_API_KEY`.

---

## Phase 6: Wiring + Build Gates

- [ ] **T013** Rewrite `libs/ai/src/lib.rs` — re-exports
      (`Tier`, `LlmClient`, `StubClient`, `RealClient`, `Profile`,
      `Resolver`, `AiError`, `CompletionMetadata`).
- [ ] **T014** `cargo check -p terrashift-ai --tests` clean.
- [ ] **T015** `cargo clippy --workspace --all-targets -- -D warnings`
      clean.
- [ ] **T016** `cargo fmt -- --check` clean.
- [ ] **T017** `cargo test --workspace` — 7 new offline tests pass;
      1 `#[ignore]`d test present.

---

## Phase 7: Review + Commit

- [ ] **T018** Self-review (reviewer agents quota-limited; checklist
      from `code-reviewer` + `constitution-checker` prompts walked
      manually).
- [ ] **T019** Write `analyze.md` + `checklist.md` (post-impl).
- [ ] **T020** Update `SESSION_PLAN.md` row 4b — note structural
      ship; integration test pending `GROQ_API_KEY`.
- [ ] **T021** Commit `feat(p-03): LLM client + 5-layer BYOK resolver
      (S4b structural; Groq integration test gated on GROQ_API_KEY)`.
