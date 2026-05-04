# Implementation Plan: LLM client + 5-layer BYOK model resolution

**Branch**: `003-llm-byok-resolution`
**Date**: 2026-05-02
**Spec**: `specs/003-llm-byok-resolution/spec.md`

## Summary

Ship the LLM-client seam for Terrashift's deterministic-with-agent-escape-hatch
pipeline. Stage 1 ships the trait (`LlmClient`), one always-available impl
(`StubClient` for tests/dev), one production impl wrapping `stakai`
(`RealClient`, only the `openai-compatible` provider type wired — Groq is
the canonical Stage 1 target), the TOML profile schema, and the 5-layer
model resolver (operator default ← profile selection ← CLI launch override
← in-conversation `/model` switch ← per-call override). Real Groq
integration test is `#[ignore]`d so default `cargo test` is offline-safe;
it activates with `--ignored` once `GROQ_API_KEY` is set.

## Technical Context

**Language/Version**: Rust 1.94.1 (per `pre-flight.md` Decision 3;
`rust-toolchain.toml` pinned)
**Primary Dependencies**: `stakai = "0.3"` (workspace dep, line 67 of root
Cargo.toml; Stakpak's published Apache-2 LLM SDK), `serde`, `toml`,
`tokio`, `async-trait`, `thiserror`, `tracing`. All present in
`libs/ai/Cargo.toml`.
**Storage**: N/A (the LLM client itself is stateless; profile config
is read from `~/.terrashift/config.toml` once at startup)
**Testing**: `cargo test -p terrashift-ai` (offline by default;
`--ignored` activates the real-Groq integration test)
**Target Platform**: cross-platform (Linux musl x86_64 + macOS arm64
per `pre-flight.md` Decision 3 + P-13 release pipeline)
**Project Type**: library crate within the workspace
**Performance Goals**: `Resolver::resolve_for_tier` is microseconds
(pure HashMap lookup over the parsed profile). `RealClient::complete`
is bounded by the LLM provider's latency (~1s for Groq's
`llama-3.3-70b-versatile`).
**Constraints**: Production paths Article XIII rule 3 clean
(no unwrap/expect/string-slice). Single-binary distribution
(rustls-tls only — no OpenSSL).
**Scale/Scope**: ~400 LOC production + ~250 LOC tests; one new module
under `libs/ai/src/`.

## Constitution Check

*GATE: Must pass before implementation.*

- ✓ **Article I** — Stage 1 ships zero new agents; the `LlmClient` is a
  single-call structured-output utility, not an agent loop.
- ✓ **Article II** — `stakai` is Stakpak's published crate; using it
  faithfully (no fork, no reimplementation). Cite `stakpak_arch.md §10`
  in `client.rs` head comment.
- ✓ **Article IV** — every error in `AiError` named (no anonymous
  strings). Empty prompt + missing key + unknown provider all loud.
- ✓ **Article V** — `api_key_env` references only; `RealClient` reads
  the env var at request time, never holds the value as instance
  state. `CompletionMetadata` carries `provider`/`model_id`/`endpoint`
  for `AuditPayload::LlmCall` population.
- ✓ **Article XII rule 3** — components bind to `Tier`, not specific
  models. `Tier::Eco` and `Tier::Smart` resolve to concrete models per
  the profile.
- ✓ **Article XIII rule 3** — production paths clippy-clean.
- ✓ **Article XIII rule 4** — `stakai` provides `Message`/`Role` typed
  surface; we don't redefine `ChatMessage`/`LLMMessage` in `libs/ai`.

## Project Structure

### Documentation (this feature)

```text
specs/003-llm-byok-resolution/
├── plan.md           # This file
├── spec.md           # Feature spec (User Stories + FRs + SCs)
├── tasks.md          # Phase-organized task list
├── analyze.md        # Post-impl notes (created by /speckit-analyze equiv)
└── checklist.md      # Final gate checklist
```

### Source Code (repository workspace)

```text
libs/ai/
├── Cargo.toml         # already declares stakai + tokio + serde + toml
└── src/
    ├── lib.rs         # re-exports
    ├── tier.rs        # Tier enum (Eco | Smart)
    ├── errors.rs      # AiError enum
    ├── profile.rs     # Profile + ProviderConfig TOML schema
    ├── resolver.rs    # 5-layer model resolver
    ├── client.rs      # LlmClient trait + StubClient
    ├── real.rs        # RealClient wrapping stakai (openai-compatible only)
    └── metadata.rs    # CompletionMetadata for AuditPayload::LlmCall

libs/ai/tests/
└── llm_client_test.rs   # Stage 1 offline tests
                         # + #[ignore]'d real_groq_completes
```

**Structure Decision**: standard Rust library crate; one module per
concern (`tier`, `errors`, `profile`, `resolver`, `client`, `real`,
`metadata`). Mirrors Stakpak's `libs/ai/src/` shape per
`stakpak_arch.md §10` — one file per orthogonal axis.

## Complexity Tracking

> No Constitution Check violations. No additional complexity beyond the
> Stage 1 minimal scope (trait + 2 impls + resolver + profile parser).
> Stage 2+ adds: streaming completions, tool calling, multi-provider
> registry, fallback chains. Those layers thread through `LlmClient`
> additively.
