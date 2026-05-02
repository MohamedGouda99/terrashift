# Implementation Plan: Mapper — single-LLM-call structured output

**Branch**: `005-mapper`
**Date**: 2026-05-02
**Spec**: `specs/005-mapper/spec.md`

## Summary

Ship Mapper structural code that consumes `LlmClient` (P-03) via
`StubClient`, queries `KnowledgeService` (P-07) for RAG hits, runs
the message stream through a `ContextReducer` (Article XIII rule 1),
caches by inventory hash (Article XII rule 2), and returns a typed
`MappingPlan` ready for Validator (P-06) gating + Generator (P-08)
HCL emit. Real Groq round-trip is one `RealClient`-vs-`StubClient`
swap away once `GROQ_API_KEY` is set.

## Technical Context

**Language/Version**: Rust 1.94.1
**Primary Dependencies**: `terrashift-ai` (P-03), `terrashift-knowledge`
(P-07), `serde`, `serde_json`, `sha2`, `tokio`, `async-trait`,
`thiserror`, `tracing`. All present in `libs/engine/Cargo.toml`.
**Storage**: in-process `HashMap<String, MappingPlan>` for the cache;
disk-backed cache is S5+.
**Testing**: `cargo test -p terrashift-engine --test mapper_test`
(offline; uses `StubClient` from `terrashift-ai`).
**Target Platform**: workspace library crate.
**Project Type**: library extending the existing `libs/engine/` crate.
**Performance Goals**: Mapper happy path is bounded by the LLM call
(~1s for Groq's `llama-3.3-70b-versatile`); cache hit is microseconds.
**Constraints**: Article XIII rule 3 (no unwrap/expect/string-slice in
production). No `response_format` available in stakai 0.3 (per
reference-explorer), so Stage 1 uses prompt-instructed JSON.
**Scale/Scope**: ~500 LOC production + ~300 LOC tests.

## Constitution Check

*GATE: Must pass before implementation.*

- ✓ **Article I** — Mapper is *not* an agent. Single LLM call, single
  parse, return. No loops, no retry. Stakpak's "LLM with structured
  output" pattern (`stakpak_arch.md §40`).
- ✓ **Article III** — output validated against the `MappingPlan`
  schema (via `serde_json::from_str::<MappingPlan>`). Non-conforming
  → `MapperError::MalformedResponse`. Validator (P-06) gates downstream.
- ✓ **Article IV** — every `MapperError` variant named.
- ✓ **Article XII rule 1** — empty inventory short-circuits without
  an LLM call. Per-call ceiling enforced by upstream cap (S4-close).
- ✓ **Article XII rule 2** — cache by `Sha256(serde_json::to_vec(estate))`.
- ✓ **Article XIII rule 1** — `reducer: &dyn ContextReducer` is in the
  function signature; type system makes bypass impossible.
- ✓ **Article XIII rule 3** — production paths grep clean.
- ✓ **Article XIII rule 4** — we don't redefine stakai's `Message`/`Role`;
  Stage 1's `Message` is a thin local struct that's TODO'd to
  converge with stakai's once `LlmClient::complete` evolves to
  carry typed messages (S5+).

## Project Structure

### Documentation (this feature)

```text
specs/005-mapper/
├── plan.md           # this file
├── spec.md           # User Stories + FRs + SCs
├── tasks.md          # Phase-organized task list
├── analyze.md        # post-impl notes
└── checklist.md      # final gate
```

### Source Code (repository workspace)

```text
libs/engine/src/mapper/
├── mod.rs            # UPDATE: Mapper struct + map() fn + re-exports
│                       (existing types MappingPlan, MappedResource,
│                       AttributeValue stay; commit 07d26b3)
├── context.rs        # NEW: ContextReducer trait + PassthroughContextReducer
├── prompt.rs         # NEW: SYSTEM_PROMPT const + build_user_prompt()
├── cache.rs          # NEW: estate_cache_key() + MapperCache
└── errors.rs         # NEW: MapperError enum
                       # (existing MapperLookupError stays in mod.rs)

libs/engine/tests/
└── mapper_test.rs    # NEW: 8+ offline tests with StubClient + counting stubs
```

**Structure Decision**: extend the existing `libs/engine/src/mapper/`
module; do NOT split into a new crate (the Mapper is engine-internal
machinery, not a public crate boundary). Pattern matches Scanner
(`libs/engine/src/scanner/`).

## Complexity Tracking

> No Constitution Check violations.
>
> One deliberate deviation from Stakpak's pattern: `ContextReducer`
> trait signature is narrower (drops `model`/`max_output_tokens`/
> `tools`/`metadata` arguments). The full signature only earns its
> keep when `BudgetAwareContextReducer` ships in S9. Documented
> inline in `context.rs` head comment so the S9 widening is
> straightforward.
