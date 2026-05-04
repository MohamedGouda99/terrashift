# Feature Specification: LLM client + 5-layer BYOK model resolution

**Feature Branch**: `003-llm-byok-resolution`
**Created**: 2026-05-02
**Status**: Draft (S4b structural; integration test gated on `GROQ_API_KEY`)
**Input**: P-03 from `terrashift_prompts.md` lines 156-219 (canonical prompt
quoted verbatim in Appendix A below).
**Reference**: `terrashift_plan.md` §6.3 (LLM Router design),
`Terrashift_Plan.docx` §6.3 (same content, longform). Source pattern
crate: `stakai = "0.3"` (Stakpak's published LLM SDK; workspace dep
already declared in `Cargo.toml:67`).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator chooses their LLM provider via TOML profile (Priority: P1)

The Terrashift operator runs `terrashift migrate` on their workstation.
Their `~/.terrashift/config.toml` carries a `[profiles.default]` block
with `tiers.eco = "groq/llama-3.3-70b-versatile"` and a
`[profiles.default.providers.groq]` block declaring
`api_endpoint = "https://api.groq.com/openai/v1"` +
`api_key_env = "GROQ_API_KEY"`. The Mapper, when it eventually fires
in S4, hits Groq's free tier with the operator's own key — Terrashift
never sees a Vodafone/Anthropic/OpenAI bill that the operator didn't
authorize. Article V invariant.

**Why this priority**: BYOK is the foundational trust property of
Terrashift's commercial model. Without it, customers can't run the
tool against their own Vodafone/internal LLM gateway and we'd be
locked to a single vendor. P-03 ships the *seam* even though the
real Groq integration test fires only when `GROQ_API_KEY` is set.

**Independent Test**: Load a `Profile` from a fixture TOML;
`Resolver::resolve_for_tier(&profile, Tier::Eco)` returns the
correct `(provider_key, model_id, api_endpoint, api_key_env)` tuple.
No network call required.

**Acceptance Scenarios**:
1. **Given** a profile TOML with `tiers.eco = "groq/llama-3.3-70b-versatile"`, **When** the resolver is asked for `Tier::Eco`, **Then** it returns `provider_key = "groq"` + `model_id = "llama-3.3-70b-versatile"` + the resolved endpoint + the resolved api_key_env.
2. **Given** a profile with no `[providers.<key>]` block matching the tier model's `provider_key`, **When** resolved, **Then** `Resolver` returns `Err(AiError::UnknownProvider)` with the missing key named (Article IV).
3. **Given** a CLI override `--model anthropic/claude-opus-4-7`, **When** resolved, **Then** the override wins over the profile default per the 5-layer priority chain.

### User Story 2 — Tests run hermetically with `StubClient` (Priority: P1)

When P-05 (Mapper) lands in S4 and uses `LlmClient`, its tests must
run without a real LLM call. The Mapper test suite injects
`StubClient` with canned `(prompt → response)` pairs so the test is
deterministic and offline.

**Why this priority**: Stage 1's eval suite (P-12, 5 goldens) +
S4's Mapper integration tests both need a stable LLM stub. Without
it, `cargo test --workspace` hangs on Groq rate limits. Article XII
rule 4 (regression gate) depends on deterministic offline tests.

**Independent Test**: Construct a `StubClient` with one canned
response, call `client.complete(Tier::Eco, "any prompt").await`,
assert the response matches.

**Acceptance Scenarios**:
1. **Given** `StubClient::new().with_response("foo", "bar")`, **When** `complete(Tier::Eco, "foo")`, **Then** result is `Ok("bar")`.
2. **Given** `StubClient::new().with_default("default response")`, **When** `complete(Tier::Smart, "anything not seeded")`, **Then** result is `Ok("default response")`.

### User Story 3 — Real `stakai`-backed Client wires when `GROQ_API_KEY` is set (Priority: P2)

When the operator sets `GROQ_API_KEY` in their environment, the
`#[ignore]`d integration test (`real_groq_completes`) fires and
proves `RealClient` actually round-trips through stakai → Groq's
openai-compatible endpoint and gets a non-empty response.

**Why this priority**: Closes S4b. Without this test, S4 stays
yellow until someone manually verifies. With it, CI pipeline can
flip the `#[ignore]` to active once GitHub Actions has the secret.

**Independent Test**: `BOOTSTRAP_GROQ=1 cargo test -p terrashift-ai
real_groq_completes -- --ignored` against a live `GROQ_API_KEY`.

**Acceptance Scenarios**:
1. **Given** `GROQ_API_KEY` is exported, **When** the ignored integration test runs, **Then** `RealClient::complete(Tier::Eco, "ping")` returns `Ok(non_empty_string)` within 30 seconds.

### Edge Cases

- **Empty prompt** — `complete(tier, "")` is rejected with `AiError::EmptyPrompt` (Article IV).
- **Missing api_key_env at runtime** — `Resolver::resolve_for_tier` succeeds (returns the env var name) but `RealClient::complete` returns `AiError::MissingApiKey { env_var }` when it tries to read it.
- **Unknown tier on profile** — `resolve_for_tier(Tier::Smart)` against a profile that omits `tiers.smart` falls back to `tiers.eco` with a `tracing::warn!` (Stage 1 simplification; Stage 5+ may make this strict).
- **Conflicting CLI override + per-call override** — per-call wins (5-layer priority).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support a `Tier` enum with at least `Eco` and `Smart` variants. Components (Mapper, Recovery agent, etc.) bind to a tier, not a specific model.
- **FR-002**: System MUST define an `LlmClient` trait with a single async method `complete(tier: Tier, prompt: &str) -> Result<String, AiError>`. Streaming + tool-calling are deferred to Stage 2+.
- **FR-003**: System MUST ship a `StubClient` impl of `LlmClient` for offline / deterministic tests, with `with_response(prompt_substring, response)` builder and a default-response fallback.
- **FR-004**: System MUST parse a `Profile` TOML schema with these fields: `model: Option<String>`, `tiers: { eco: String, smart: Option<String> }`, `providers: Map<String, ProviderConfig>`. `ProviderConfig` carries `type` (`"openai-compatible"` / `"anthropic"` / etc.), `api_endpoint`, `api_key_env`.
- **FR-005**: System MUST implement a `Resolver` that, given a `Profile` + optional CLI override + optional per-call override, returns the concrete `(provider_key, model_id, api_endpoint, api_key_env)` tuple per the 5-layer priority chain (`terrashift_plan.md §6.X`): operator default ← profile selection ← CLI launch override ← in-conversation `/model` switch ← per-call override.
- **FR-006**: System MUST expose a `RealClient` that wraps `stakai::Inference` and translates `Tier` + prompt → stakai's `GenerateRequest`. Stage 1: only the `openai-compatible` provider type wired (Groq is one). Other types return `AiError::UnsupportedProviderType` until S2+.
- **FR-007**: All errors MUST be named in an `AiError` enum (no anonymous strings). Article IV.
- **FR-008**: The integration test `real_groq_completes` MUST be `#[ignore]`d so default `cargo test` is offline-safe; it activates with `cargo test -- --ignored` when `GROQ_API_KEY` is set.
- **FR-009**: All audit-relevant LlmCall fields (`provider`, `model_id`, `provider_endpoint`, `tier`, `input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_write_tokens`, `cost_usd_micros`) MUST be exposed on a `CompletionMetadata` struct returned alongside the response, so when S5 wires audit emission it can populate `AuditPayload::LlmCall` with no shape change. Article V invariant.

### Key Entities

- **`Tier`** — enum bound to component (`Eco | Smart`); concrete model resolved at runtime.
- **`Profile`** — operator-level config block from `~/.terrashift/config.toml`; carries default model, per-tier model assignments, provider blocks.
- **`ProviderConfig`** — per-provider block: `type`, `api_endpoint`, `api_key_env`. Stage 1 only `openai-compatible` wired.
- **`Resolver`** — pure-function struct that takes `(profile, cli_override, call_override, tier)` and returns the concrete model + endpoint + key reference.
- **`LlmClient`** — async trait. Stage 1 impls: `StubClient`, `RealClient`.
- **`AiError`** — `thiserror`-derived enum: `UnknownProvider`, `UnsupportedProviderType`, `MissingApiKey`, `EmptyPrompt`, `Stakai(stakai::Error)`, `TomlParse(...)`.
- **`CompletionMetadata`** — token counts + provider + model_id + endpoint, returned with each completion for audit population.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test -p terrashift-ai` runs ≥ 7 unit tests covering FR-001 through FR-007 and SC-002 through SC-005 below; all pass without network access.
- **SC-002**: `Resolver` round-trips a fixture TOML through serde; the resolved `(provider_key, model_id, endpoint, api_key_env)` tuple matches the fixture's expected values byte-for-byte.
- **SC-003**: `StubClient` returns canned responses deterministically; the same `(prompt, seed)` always produces the same output (Article VI).
- **SC-004**: `AiError` variants are exhaustively pattern-matched in tests covering `UnknownProvider`, `MissingApiKey`, `EmptyPrompt`. Each error message names the offending input (Article IV).
- **SC-005**: 5-layer priority chain is verified by 5 tests, one per priority level, where the higher layer's value wins over the lower layer's.
- **SC-006**: Real Groq integration test (`#[ignore]`d) fires successfully when `GROQ_API_KEY` is set and returns a non-empty response within 30 seconds.
- **SC-007**: Production paths in `libs/ai/src/` are clippy `unwrap_used` / `expect_used` / `string_slice` clean (Article XIII rule 3).

## Assumptions

- **Workspace dep `stakai = "0.3"` is already declared** in the root `Cargo.toml` (verified line 67). Adding `libs/ai/Cargo.toml` already pulls it.
- **`~/.terrashift/config.toml`** is the canonical operator config; per pre-flight Decision 10's Groq sample TOML, the schema below is what real operators will write.
- **`tracing` is already in workspace deps**; LLM call spans use `tracing::info_span!` with structured fields (`provider`, `model_id`, `tier`, `input_tokens`, `output_tokens`, `latency_ms`).
- **Real Mapper integration with this client lands in S4 (P-05)**, not P-03. P-03 only ships the LLM seam.

## Appendix A — Canonical P-03 prompt (verbatim from `terrashift_prompts.md`)

```text
P-03 — LLM call via stakai

Implement libs/ai. Wraps stakai::Inference. Tier-aware
complete(Tier::Eco | Smart, prompt). 5-layer model resolution
(profile → CLI flag → /model → per-call override). Provider
auto-registration (silent omission when no API key). Policy /
preference field split per Article V. ~/.terrashift/config.toml
schema:

  [profiles.default]
  model = "groq/llama-3.3-70b-versatile"

    [profiles.default.tiers]
    eco   = "groq/llama-3.3-70b-versatile"
    smart = "groq/llama-3.3-70b-versatile"

    [profiles.default.providers.groq]
    type = "openai-compatible"
    api_endpoint = "https://api.groq.com/openai/v1"
    api_key_env = "GROQ_API_KEY"

Integration test: real Groq API call to llama-3.3-70b-versatile.
#[ignore]'d so default `cargo test` is offline-safe.

CONSTITUTION CHECKS:
- Article V (api_key_env, never raw key in code; CompletionMetadata
  carries provider/model_id/endpoint for AuditPayload::LlmCall)
- Article XII rule 3 (tier-bound, not model-bound)
- Article XIII rule 3 (no unwrap/expect/string-slice in production)
- Article XIII rule 4 (ChatMessage vs LLMMessage discipline — handled
  by stakai's typed surface; we don't redefine those types)
```

## Appendix B — Why this spec uses the official template

Specs 000-014 use a custom Goal/Scope shape — those P-NNs were
hand-specified without the `.specify/templates/spec-template.md`
form because Spec Kit's `/speckit-*` slash commands were not
surfaced via the Skill tool at session start. This spec (003) is
the first to follow the template's User-Story / FR-NNN / SC-NNN
shape. Going forward, every new P-NN's spec uses this format.
