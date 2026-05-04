# Analysis — P-05 (post-implementation)

## Decisions taken at implementation time

### A. stakai 0.3 has no `response_format` — use prompt-instructed JSON

Reference-explorer audited `refs/stakpak/libs/ai/src/types/{request,options}.rs`
and confirmed: stakai 0.3 ships no `response_format` / JSON-schema
field on `GenerateRequest` or `GenerateOptions`. The Stakpak idiom for
"JSON output" is the **tool-as-output-channel pattern**: define a `Tool`
whose `function.parameters` carries the schemars-derived schema and
set `tool_choice: Required`. Reference: `refs/stakpak/libs/agent-core/src/agent.rs:284-298`.

Stage 1 deviation: `LlmClient::complete` from P-03 returns `String`,
not a typed JSON value. So Mapper:
1. Sends a strong system prompt instructing JSON-only output (no prose, no fences).
2. Calls `llm.complete(Tier::Eco, prompt)` — gets back `String`.
3. Parses with `serde_json::from_str::<MappingPlan>`.
4. On parse failure: `MapperError::MalformedResponse { reason, sample }`.

**When to revisit**: if eval signal (P-12 token-cost gate, S7 expansion) shows malformed-JSON rates >5%, S5+ adopts the tool-call channel pattern. Until then, the simpler path is a deliberate Stage 1 trade-off documented inline in `prompt.rs`.

### B. `ContextReducer` narrower signature than Stakpak's

Stakpak's `ContextReducer::reduce` takes 5 parameters (`messages`,
`model`, `max_output_tokens`, `tools`, `metadata`). Stage 1 narrows to
just `messages: Vec<Message> -> Vec<Message>`. The narrowing is
documented inline at the head of `mapper/context.rs` so the S9
widening (when `BudgetAwareContextReducer` ships with the agent loop
kernel) is purely additive.

### C. Stage-1-local `Message` type

`Message { role: Role, content: String }` is a Stage-1 convenience
because `LlmClient::complete` from P-03 takes `&str`, not typed
messages. Stakai's `Message`/`Role` are the canonical types
(Article XIII rule 4); we don't redefine `ChatMessage` /
`LLMMessage` from `libs/shared`. When `LlmClient` evolves to take
typed messages (S5+), this local `Message` either disappears or grows
a `From<stakai::Message>` impl — the `ContextReducer` trait stays
unchanged because it's typed against the local `Message`.

### D. `truncate_utf8` helper for `MalformedResponse` sample

Article XIII rule 3 forbids `&s[..n]` slice indexing in production
(panics on non-UTF-8 boundaries). The `truncate_utf8(s, max_bytes)`
helper walks backward from `max_bytes` until `is_char_boundary` then
returns `s.get(..end).unwrap_or("")`. The `unwrap_or("")` is
`Option::unwrap_or` (deliberate fallback), allowed per workspace
clippy.toml.

### E. `MAX_INVENTORY_BYTES = 100_000` (100 KB)

Stage 1 fails loudly on inventories > 100 KB serialized (Article IV).
Real-world Terraform repos can produce multi-MB inventories — Stage 2+
adds chunking. The 100 KB cap covers the Stage 1 demo (15 resources,
~2 KB) with substantial headroom; Pratik fixture (~30 resources)
serializes to ~5 KB. Cap deliberately tight to surface the chunking
need clearly when we hit a real customer estate.

## Stakpak / Claude Code reference fidelity

- **Verbatim mirror**: `Sha256::new() → update(bytes) → format!("{:x}", finalize())` cache-key idiom from `refs/stakpak/tui/src/services/plan.rs:137-141`.
- **Verbatim mirror**: `ContextReducer` trait shape (with documented narrowing) from `refs/stakpak/libs/agent-core/src/context.rs:8-17`.
- **Verbatim mirror**: `PassthroughContextReducer` no-op shape from Stakpak's `PassthroughCompactionEngine` at `refs/stakpak/libs/agent-core/src/compaction.rs:22-45`.
- **Adapted (canonical happy path)**: `agent.rs:159-187`'s `reduce → generate` sequence — Mapper's body inlines this pattern in a Stage 1 single-call form.

Claude Code refs not consulted for P-05 — this is a Stakpak-pattern domain, not a slash-command/UI domain.

## What's NOT here (Stage 1 deferrals)

- **Real Groq integration test for the full Mapper happy path** — P-03's `real_groq_completes` is `#[ignore]`d; once GROQ_API_KEY is set, a Mapper-level integration test would round-trip `Mapper::map(... &RealClient ...)`. Wired in S4-close.
- **Tool-call channel for structured output** — see §A above.
- **`BudgetAwareContextReducer`** — full reducer with the six-pass pipeline (`reduce_context`) lives in S9 with the agent loop kernel.
- **`CompactionEngine` on the Mapper path** — `CompactionEngine` is Stakpak's overflow-recovery escape hatch (`agent.rs:193-219`). Single-LLM-call Mapper has no use for it; if inventory is too big, Stage 1 fails loudly per `MAX_INVENTORY_BYTES`.
- **Disk-backed cache** — Stage 1's in-process `HashMap` is fine for single-binary CLI; multi-tenant SaaS in S34 may add SQLite-backed cache.
- **Token cost in `MapperError`** — when LLM call fails partway, we'd want to attribute partial tokens to the operator's bill. Defer until `RealClient` actually populates token counts in `CompletionMetadata`.
- **Retry on malformed JSON** — `MapperError::MalformedResponse` is loud + final. A retry budget could narrow malformed-JSON rates but adds complexity disproportionate to evidence; defer until eval signal warrants.

## Spec criteria coverage

All 7 numbered criteria from spec.md `## Success Criteria` have a test:

| Criterion | Test |
|---|---|
| SC-001 7+ offline tests | 7 tokio::tests + 1 sync test |
| SC-002 happy path round-trip | `happy_path_with_stub_client` |
| SC-003 cache hit avoids LLM | `cache_hit_avoids_llm_and_reducer` |
| SC-004 reducer on path | `reducer_is_on_path` |
| SC-005 malformed loud | `malformed_json_is_loud_error` |
| SC-006 cache key stable | `estate_cache_key_is_stable_across_runs` |
| SC-007 clippy clean | enforced by workspace gates |

Bonus tests:
- `empty_inventory_skips_llm_call` (SC: Article XII rule 1)
- `prompt_contains_inventory_resources` (SC: knowledge hits + system prompt)

## eval-runner agent confirmation

Ran `eval-runner` after implementation — all 5 goldens still pass.
Mapper is **off the goldens' path** (P-12 design: goldens pre-curate
`mapping_plan.json`, drive `Generator::generate(plan)` directly), so
P-05 changes don't affect goldens. The agent confirmed:
- 6 tests pass, 1 ignored (`bootstrap_goldens`)
- Article VI determinism preserved
- Article IV loud-failure invariants intact
- No workspace-wide regressions

Verdict: GO.
