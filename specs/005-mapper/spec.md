# Feature Specification: Mapper — single-LLM-call structured output

**Feature Branch**: `005-mapper`
**Created**: 2026-05-02
**Status**: Draft (S4b structural; LLM call defaults to `StubClient`;
real Groq route fires when wired with `RealClient` from P-03 and
`GROQ_API_KEY` is set).
**Input**: P-05 from `terrashift_prompts.md` lines 352-385 (verbatim
in Appendix A).
**Reference**: `terrashift_plan.md` §5.1 (Mapper is "LLM with structured
output" — NOT an agent), `Terrashift_Plan.docx` §5.1 same content
longform. Source patterns: `refs/stakpak/libs/agent-core/src/context.rs`
(`ContextReducer` trait + `DefaultContextReducer`),
`refs/stakpak/libs/agent-core/src/agent.rs:159-187` (canonical
`reduce → generate` happy path), `refs/stakpak/tui/src/services/plan.rs:137-141`
(`Sha256` cache-key idiom).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Mapper produces a valid `MappingPlan` from an `EstateInventory` (Priority: P1) 🎯 MVP

The CLI scans a Terraform repo with the Scanner (P-04), gets an
`EstateInventory`, and asks the Mapper to translate it to the target
cloud. Mapper runs ONE LLM call returning JSON conforming to the
`MappingPlan` schema, parses it, and returns the plan. Validator
(P-06) then gates it before Generator (P-08) emits HCL.

**Why this priority**: This is the core value of Stage 1's MVP demo —
turning a GCP estate into an AWS migration plan. Without the Mapper,
the deterministic pipeline has no input. Article I scope:
*not* an agent, single LLM call, then deterministic.

**Independent Test**: Construct an `EstateInventory` fixture (1-3
resources), wire `StubClient` with a canned `MappingPlan` JSON
response, call `Mapper::map(...)`, assert the parsed plan equals the
canned content.

**Acceptance Scenarios**:
1. **Given** an `EstateInventory` with one `google_compute_network.main` resource and a `StubClient` returning a canned `MappingPlan` JSON for `aws_vpc.main`, **When** `Mapper::map(estate, knowledge, llm, reducer, cache)`, **Then** result is `Ok(MappingPlan)` with one `MappedResource` whose `target_addr == "aws_vpc.main"`.
2. **Given** the same input, **When** `map(...)` is called twice, **Then** the second call's result is byte-identical to the first AND the `LlmClient` was invoked **only once** (cache hit on inventory hash).
3. **Given** a `StubClient` returning malformed JSON (`"{not valid"`), **When** `map(...)`, **Then** result is `Err(MapperError::MalformedResponse { reason, sample })` (Article IV — loud, names the parse error + a truncated raw sample).

### User Story 2 — Article XIII rule 1: every Mapper run goes through `ContextReducer::reduce` (Priority: P1)

Stakpak's most-bitten anti-pattern (rule 1, `stakpak_arch.md §42`):
bypass the `ContextReducer` and Anthropic returns 400 on dangling
`tool_use` blocks. Terrashift adopts the discipline from day one,
even though Stage 1 messages are simple — the seam is on the path so
Stage 2+ extensions can swap a real reducer in without touching the
Mapper code.

**Why this priority**: The discipline is cheap to ship now and
catastrophic to retrofit. P-05 enforces it via type signature:
`map(...)` requires a `&dyn ContextReducer` parameter; there is no
non-reducer path.

**Independent Test**: Wire a `CountingContextReducer` test impl that
increments a counter on every `reduce()` call. Assert the counter is
1 after `Mapper::map(...)`.

**Acceptance Scenarios**:
1. **Given** a `CountingContextReducer` test stub with `count: AtomicUsize`, **When** `Mapper::map(...)` is called once, **Then** `count.load() == 1` (reducer is on the path).
2. **Given** a `Mapper::map(...)` invocation that hits a cached result, **When** the cache hit returns, **Then** `reduce()` is **NOT** called (cache short-circuits before the LLM step entirely — Article XII rule 2).

### User Story 3 — Knowledge hits inform the prompt (Priority: P2)

The Mapper queries `KnowledgeService::find_similar_in_provider` for
each source resource type to retrieve the top-K target candidates,
then includes those in the LLM prompt as few-shot context. Per
`terrashift_plan.md §6.3`, this is what gets the AWS provider's
~1,400 resources into the 128K-context Llama-3.3-70B window.

**Why this priority**: Without RAG hits, the Mapper either has to
embed the entire target schema (won't fit) or hallucinate (Article
III violation). RAG is the bridge.

**Independent Test**: With `StubClient` capturing the prompt it
receives, assert that the prompt body contains the resource type of
each top-K knowledge hit returned by a stub `KnowledgeService`.

**Acceptance Scenarios**:
1. **Given** a stub `KnowledgeService` returning `["azurerm_virtual_network", "azurerm_subnet"]` for source `aws_vpc`, **When** `Mapper::map(...)` runs, **Then** the prompt sent to `LlmClient::complete` contains the strings `azurerm_virtual_network` AND `azurerm_subnet`.

### Edge Cases

- **Empty `EstateInventory`** — `map()` returns an `Ok(MappingPlan)` with empty `resources`, no LLM call (saves tokens).
- **Inventory > 1MB serialised** — Stage 1 fails loudly with
  `MapperError::InventoryTooLarge` (Article IV); Stage 2+ adds chunking.
- **`KnowledgeService` returns zero hits** — Mapper proceeds with no few-shot context; the system prompt still grounds the request, and Validator (P-06) catches any hallucinations downstream.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST expose `Mapper::new(...)` + `async fn map(estate, knowledge, llm, reducer, cache) -> Result<MappingPlan, MapperError>`.
- **FR-002**: System MUST define `pub trait ContextReducer { fn reduce(&self, messages: Vec<Message>) -> Vec<Message>; }` and ship `PassthroughContextReducer` as the Stage 1 default. Mirror `refs/stakpak/libs/agent-core/src/context.rs:8-17` shape; narrow the signature (drop `model`/`max_output_tokens`/`tools`/`metadata` until S9). The narrowing is documented inline.
- **FR-003**: `map()` MUST route every non-cached call through `reducer.reduce(messages)` BEFORE invoking `llm.complete`. The reducer parameter is non-optional in the function signature (Article XIII rule 1 enforced via type system).
- **FR-004**: System MUST cache `MappingPlan` results by a SHA-256 hash of the `EstateInventory`'s canonical JSON serialisation. Cache hits return the cached plan WITHOUT calling `reducer.reduce` or `llm.complete` (Article XII rule 2).
- **FR-005**: `MapperError` MUST name every failure mode: `EmptyKnowledge` (informational; not blocking), `MalformedResponse { reason, sample }`, `InventoryTooLarge { bytes }`, `Llm(AiError)`, `Knowledge(KnowledgeError)`, `Serialize(serde_json::Error)` (boxed for clippy `result_large_err`).
- **FR-006**: Mapper MUST query `KnowledgeService::find_similar_in_provider` per unique source resource type and include the top-K target candidates in the LLM prompt (FR-007). Stage 1: `top_k = 5`.
- **FR-007**: System MUST construct the prompt as: (system) "You are a Terrashift Mapper. Output ONLY valid JSON conforming to the MappingPlan schema. No prose." + (user) `<source_provider>` + `<target_provider>` + `<resource_list_serialised>` + `<knowledge_hits_per_resource>`.
- **FR-008**: When the LLM response is not valid JSON for `MappingPlan`, `map()` MUST return `MapperError::MalformedResponse` with the underlying `serde_json::Error` reason and the first 500 bytes of the raw response (Article IV).
- **FR-009**: Empty `EstateInventory` MUST short-circuit to `Ok(MappingPlan { resources: vec![], ... })` without calling the LLM (saves tokens; Article XII rule 1).

### Key Entities

- **`Mapper`** — orchestrator struct holding the active provider name + Stage 1 pipeline state (mostly stateless).
- **`MapperCache`** — in-process `HashMap<String, MappingPlan>` keyed by `estate_cache_key(&EstateInventory)`. Stage 1: no eviction.
- **`ContextReducer`** trait + `PassthroughContextReducer` impl — Article XIII rule 1 seam.
- **`MapperError`** — `thiserror`-derived enum.
- **`MapperPrompt`** — pure-fn module producing the `(system, user)` strings.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test -p terrashift-engine --test mapper_test` runs ≥ 7 offline tests covering FR-001 through FR-009; all pass without network access.
- **SC-002**: `Mapper::map(...)` round-trips a canned `MappingPlan` JSON through `StubClient` and returns the same `MappingPlan` (`assert_eq` on the deserialized struct).
- **SC-003**: Cache hit verified — second `map()` call with same `EstateInventory` invokes `LlmClient::complete` zero additional times. Verified via `CountingLlmClient` test stub.
- **SC-004**: `ContextReducer::reduce` is invoked exactly once per non-cached `map()` call. Verified via `CountingContextReducer`.
- **SC-005**: Malformed JSON response surfaces `MapperError::MalformedResponse` with the reason + a sample. Verified.
- **SC-006**: `estate_cache_key(&inventory)` is byte-stable across runs (Article VI). Verified by computing the key twice on the same inventory and asserting equality.
- **SC-007**: Production paths in `libs/engine/src/mapper/` are clippy `unwrap_used` / `expect_used` / `string_slice` clean (Article XIII rule 3).

## Assumptions

- **`LlmClient` from P-03 is wired** — `terrashift-ai = { workspace = true }` already in `libs/engine/Cargo.toml:28`.
- **stakai 0.3.x has no `response_format` field** (verified by reference-explorer). Stage 1 instructs the LLM via system prompt to return JSON; if eval signal shows malformed-JSON rates are problematic, Stage 5+ may add the tool-call channel pattern from Stakpak's `agent.rs:284-298`.
- **Real Groq integration test** is `#[ignore]`d at this layer — when `GROQ_API_KEY` is set + the test wires `RealClient` instead of `StubClient`, the round-trip closes S4 fully.
- **`KnowledgeService::find_similar_in_provider` is stable** (shipped P-07 commit `2b48216`); `top_k` defaults to 5.
- **`EstateInventory` already derives `Serialize`** (verified — Scanner P-04 commit `f61dc31`).

## Appendix A — Canonical P-05 prompt (verbatim from `terrashift_prompts.md:352-385`)

```text
P-05 — Mapper (LLM with structured output, single call)

When to use: After P-04.
Reference sections: stakpak_arch.md section 8 (agent loop), section 20
(tool call lifecycle), section 23 (context trimming with cache
preservation).

Implement the Mapper. Takes an EstateInventory (from Scanner),
produces a MappingPlan listing target resources for each source
resource.

NOT AN AGENT. Single LLM call producing structured output, then
deterministic dispatch. This is the pattern stakpak_arch.md section 40
calls "LLM with structured output" — distinct from a true agent that
loops.

PRIMARY REFERENCE: stakpak_arch.md section 10 covers stakai's
tool-call shape. We use a single tool call where the "tool" is
emit_mapping_plan with a strict JSON schema.

IMPLEMENT in libs/engine/src/mapper/mod.rs:

1. Build a JSON schema for output structure using schemars derive
2. Construct the prompt: source provider, target provider, resource
   list, few-shot examples from libs/knowledge
3. Single LLM call via libs/ai's complete_with_tools
4. Validate response against schema; fail loudly per Article IV if
   non-conforming
5. Cache the result by input hash per Article XII rule 2

CROSS-CUTTING: Read stakpak_arch.md section 23 (context trimming with
cache preservation). Mapper prompts must respect cache-stability
invariant — once shipped, changes invalidate every cached mapping.

CONSTITUTION CHECKS:
- Article I (LLM-with-structured-output, not an agent)
- Article III (output validated against schema)
- Article XII (cached, regression-gated)
- Article XIII rule 1 (don't bypass message conversion)
```

**Stage 1 deviation from the prompt**: stakai 0.3.x has no
`response_format` field and `LlmClient::complete` from P-03 returns
`String`, not a typed JSON value. So Stage 1 uses prompt-instructed
JSON output (system prompt asks for valid JSON, response is parsed
via `serde_json::from_str`). The tool-call workaround from
`refs/stakpak/libs/agent-core/src/agent.rs:284-298` is the canonical
production path; deferred to S5+ if eval signal warrants. The
structural Mapper code with `StubClient` is fully shippable today
and the swap to the tool-call path is a `LlmClient` extension, not
a Mapper change.

## Appendix B — Spec format

This is the second P-NN's spec to use the official Spec Kit
template (after P-03). Specs 000–014 use the legacy Goal/Scope shape;
the format change is documented in `SESSION_PLAN.md` and applies
prospectively only.
