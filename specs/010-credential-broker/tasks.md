---
description: "Task list for P-10 Credential broker structural"
---

# Tasks: Credential broker — Article V cornerstone

**Input**: `specs/010-credential-broker/{spec,plan}.md`
**Tests**: required (per FR-001 through FR-009)

## Phase 1: Setup

- [ ] **T001** Verify `libs/creds/Cargo.toml` has `zeroize`,
      `terrashift-audit`, `async-trait`, `tokio`, `thiserror`,
      `tracing`. No additions needed.

## Phase 2: Foundational

- [ ] **T002** [P] Write `libs/creds/src/errors.rs` — `CredsError` enum.
- [ ] **T003** [P] Write `libs/creds/src/broker.rs` — `CredentialBroker`
      trait + `Credential` (Zeroizing wrapper).
- [ ] **T004** [P] Write `libs/creds/src/scrub.rs` — `pre_llm_check`
      thin wrapper over `terrashift_audit::scrubber::scan`.

## Phase 3: User Story 1 — Substitution + Zeroize (P1) 🎯 MVP

- [ ] **T005** [US1] Write `libs/creds/src/substitution.rs` —
      `substitute()` + `SubstitutionMap` + reverse rebuild.
- [ ] **T006** [US1] Write `libs/creds/src/stub.rs` — `StubBroker`
      with canned-credential map, audit-store-injectable via builder.
- [ ] **T007** [US1] Tests:
      - `substitution_roundtrip`
      - `substitution_unknown_secret_loud_error`
      - `substitution_multiple_refs_in_one_pass`
      - `credential_zeroizes_on_drop`

## Phase 4: User Story 2 — Pre-LLM scrubber (P1)

- [ ] **T008** [US2] Tests:
      - `pre_llm_check_blocks_aws_key`
      - `pre_llm_check_passes_clean_payload`
      - `pre_llm_check_error_does_not_leak_raw_value`

## Phase 5: User Story 3 — Cloud broker scaffolding (P2)

- [ ] **T009** [US3] Write `aws.rs`, `gcp.rs`, `azure.rs` — each impls
      `CredentialBroker`, `fetch_*` returns `NotImplementedYet { which,
      session: "S5" }`.
- [ ] **T010** [US3] Tests:
      - `aws_broker_returns_not_implemented_until_s5`
      - `gcp_broker_returns_not_implemented_until_s5`
      - `azure_broker_returns_not_implemented_until_s5`

## Phase 6: User Story 4 — Audit hook (P2)

- [ ] **T011** [US4] Wire audit emission inside `StubBroker::fetch_*`:
      injected `Option<Arc<dyn AuditStore>>`, append
      `CredentialResolution` payload on success.
- [ ] **T012** [US4] Test: `stub_broker_audits_on_fetch`.

## Phase 7: Wiring + Build Gates

- [ ] **T013** Rewrite `libs/creds/src/lib.rs` with re-exports.
- [ ] **T014** `cargo check -p terrashift-creds --tests`.
- [ ] **T015** `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **T016** `cargo fmt -- --check`.
- [ ] **T017** `cargo test --workspace`.

## Phase 8: Review + Commit

- [ ] **T018** **MANDATORY**: `security-auditor` subagent (per CLAUDE.md
      — "run before any audit/scrubber/credential-broker change").
- [ ] **T019** Self-review (code-reviewer + constitution-checker
      may be quota-limited; checklist walked manually).
- [ ] **T020** Write `analyze.md` + `checklist.md`.
- [ ] **T021** Update `SESSION_PLAN.md` row 5 — note S5 structural
      partial shipped; full S5 close awaits Docker + cloud creds.
- [ ] **T022** Commit `feat(p-10): Credential broker — Article V
      cornerstone (S5 structural; STS/ADC/managed-identity gated)`.
