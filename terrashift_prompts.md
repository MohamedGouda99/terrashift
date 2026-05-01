# Terrashift — Prompts Library

A working set of prompts for directing Opus 4.7 during Terrashift development. Keep this file open in a tab.

**How to use:**
1. Find the prompt that matches what you're trying to do.
2. Copy from `>>>` to `<<<` into Claude Code.
3. Substitute placeholder values (`<like-this>`).
4. Run.

**Order:** P-00 first (always), then P-01 (workspace setup), then by need.

**Reference document conventions.** Most prompts cite `stakpak_arch.md` by section number — e.g., `stakpak_arch.md section 8` is "stakpak-agent-core: THE CANONICAL AGENT LOOP". The architecture document at `refs/stakpak_arch.md` is the canonical reference. Stakpak source code at `refs/stakpak/` is the ground truth that backs it. Most of the time, the architecture document is enough.

---

## P-00 — Reference walkthrough and Terrashift mapping

**When to use:** First thing on day one. Once per teammate. Builds your mental model and produces the canonical mapping document.

**Preconditions:**
- `stakpak/agent` cloned at `refs/stakpak/`
- `claude-cli-src` cloned at `refs/claude-code/`
- Architecture reference at `refs/stakpak_arch.md`
- `terrashift_plan.md` in your editor

**Prompt:**

```
>>>
I am onboarding to the Terrashift project. The canonical architectural reference is `refs/stakpak_arch.md` (~2,840 lines, with file:line citations into Stakpak source). Walk me through the key sections and produce a Terrashift-specific mapping document.

PART 1 — Read these sections of stakpak_arch.md in order:

1. sections 1-4 (Part I) — Stakpak product, security thesis, user-visible modes, lifecycle
2. sections 5-7 (Part II) — workspace map, dependency graph, configuration files
3. section 8 — agent-core: THE CANONICAL AGENT LOOP. The most important section.
4. sections 9-10 — shared/api substrate and stakai LLM SDK
5. section 13 — MCP suite (config + client + server + proxy)
6. sections 18-25 (Part IV) — cross-cutting flows. All eight in sequence.
7. sections 26-30 (Part V) — security model
8. sections 39-42 (Part VIII) — the mirror playbook itself

PART 2 — Spot-read Stakpak source for the patterns you want to verify. The arch doc tells you which file:line to look at. For at least 3-5 patterns, descend into the source and confirm the doc's description matches.

PART 3 — Read Claude Code source for secondary agent-loop concepts:
1. `refs/claude-code/Tool.ts` — conceptual Tool definition
2. `refs/claude-code/QueryEngine.ts` and `query.ts` — conversation state machine
3. `refs/claude-code/commands/` — slash-command registry pattern
4. `refs/claude-code/services/compact/` — three-mode compaction

PART 4 — Produce TERRASHIFT_MAPPING.md in the workspace root:

A. The 11 seams from stakpak_arch.md section 39 in a table:
   | Stakpak seam | Terrashift equivalent | Crate / file location | Effort estimate |

B. The 13 things to replace from stakpak_arch.md section 40:
   | Stakpak domain artifact | Terrashift equivalent | Notes |

C. The 6-phase mirroring sequence from stakpak_arch.md section 41 mapped to Terrashift:
   - Phase 1 (strip identity, 1-2 days) → Stage 1 weeks 1-2
   - Phase 2 (re-skin tools, 1-2 weeks) → Stage 1 weeks 2-6
   - Phase 3 (LLM provider) → out of scope; we use stakai's existing providers
   - Phase 4 (channel adapters) → drop; no chat-platform integration
   - Phase 5 (autopilot) → drop; on-demand, not scheduled
   - Phase 6 (TUI customization) → Stage 1 baseline mirrors Stakpak's

D. Which of stakpak_arch.md section 42's 10 anti-patterns are most relevant to Terrashift's first three months and why.

E. 2-3 places where stakpak_arch.md section 42 anti-patterns map directly to Terrashift constitution articles.

F. 2-3 places where Claude Code's pattern is genuinely cleaner than Stakpak's, and we should adopt the Claude Code shape in Rust idioms.

Output: TERRASHIFT_MAPPING.md in workspace root. This becomes a permanent onboarding artifact.

Do NOT write any production code in this session.
<<<
```

---

## P-01 — Workspace bootstrap

**When to use:** Day one, after P-00. Sets up the Cargo workspace mirroring Stakpak's layout (per stakpak_arch.md section 5).

**Preconditions:**
- Empty directory: `~/projects/terrashift`
- Rust 1.75+ installed
- `terrashift_plan.md`, `TERRASHIFT_MAPPING.md` in workspace root

**Prompt:**

```
>>>
Set up the Terrashift Cargo workspace mirroring stakpak_arch.md section 5 (workspace map of 14 crates).

ROOT files:
- `Cargo.toml` — workspace declaration with [workspace.dependencies]
- `README.md` — one paragraph + link to terrashift_plan.md
- `ATTRIBUTIONS.md` — Apache 2.0 attribution for Stakpak; Claude Code source reference
- `CONSTITUTION.md` — copy section 19 of terrashift_plan.md (now includes Article XIII)
- `TERRASHIFT_MAPPING.md` — already exists from P-00
- `.github/workflows/ci.yml` — fmt + clippy + test on every PR; copy Stakpak's pattern from stakpak_arch.md section 32
- `.gitignore` — Rust standard
- `rust-toolchain.toml` — pin to 1.75 (Stakpak pins 1.94.1; we pin lower for ecosystem compatibility)

WORKSPACE MEMBERS — skeleton crates each with Cargo.toml + empty lib.rs/main.rs. Mirror Stakpak's layout from stakpak_arch.md section 5:

- `cli/` — binary
- `tui/` — Ratatui rendering
- `libs/shared/` — domain-neutral types (mirrors stakpak-shared per section 9)
- `libs/agent-core/` — Tool trait, agent loop primitives (mirrors stakpak-agent-core per section 8)
- `libs/ai/` — wrapper around stakai (the actual stakai crate is the LLM SDK)
- `libs/knowledge/` — schema cache + RAG (Terrashift-specific)
- `libs/engine/` — the 9-component migration pipeline (Terrashift-specific)
- `libs/creds/` — credential broker (mirrors Stakpak's secret handling per section 27)
- `libs/mcp/{client,server,proxy,config}/` — MCP layer (mirrors section 13 structure)
- `libs/audit/` — signed audit log (Terrashift-specific)
- `libs/eval/` — eval framework (Terrashift-specific)

WORKSPACE-LEVEL DEPENDENCIES (per stakpak_arch.md section 6):

- tokio = { version = "1", features = ["full"] }
- serde = { version = "1", features = ["derive"] }
- serde_json = "1"
- thiserror = "1"
- anyhow = "1"
- tracing = "0.1"
- tracing-subscriber = "0.3"
- reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
- ratatui = "0.29"
- crossterm = "0.29"
- clap = { version = "4", features = ["derive"] }
- toml = "0.8"
- stakai = "0.3"
- rmcp = { version = "0.11", features = ["client", "server"] }
- hcl-rs = "0.18"
- lancedb = "0.10"
- sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite", "postgres"] }
- schemars = "0.8"
- zeroize = "1"
- uuid = { version = "1", features = ["serde", "v4"] }
- chrono = { version = "0.4", features = ["serde"] }

Workspace lint posture (per stakpak_arch.md section 31):

[workspace.lints.clippy]
unwrap_used = "deny"        # Article XIII rule 3
expect_used = "deny"        # Article XIII rule 3
string_slice = "deny"       # Article XIII rule 3

After scaffolding, run `cargo check --all-targets` and confirm the workspace builds.

Do NOT implement business logic in this session. Only scaffolding.

Constitution check: cite Article II (reference codebase discipline), Article VII (repository hygiene), Article XIII rule 3.
<<<
```

---

## P-02 — Tool trait and ToolExecutor

**When to use:** First real code. After P-01.

**Reference sections:** `stakpak_arch.md section 8` (the agent loop kernel), `section 20` (tool call lifecycle).

**Prompt:**

```
>>>
Implement the Tool trait and ToolExecutor in libs/agent-core. This is the foundation that every Terrashift tool will use.

PRIMARY REFERENCE: Read stakpak_arch.md section 8 carefully. The "Key traits" subsection lists ToolExecutor as one of the 5 seams. Trait signature:

  trait ToolExecutor {
      async fn execute_tool_call(
          &self,
          run: &AgentRunContext,
          call: &ProposedToolCall,
          cancel: CancellationToken,
      ) -> Result<ToolExecutionResult>;
  }

  enum ToolExecutionResult {
      Completed { result: String, is_error: bool },
      Cancelled,
  }

CROSS-CUTTING: Read stakpak_arch.md section 20 (tool call lifecycle). Lifecycle: Build ApprovalStateMachine → next_ready → emit ToolExecutionStarted → executor → emit ToolExecutionCompleted.

DESCEND INTO SOURCE: Open `refs/stakpak/libs/agent-core/src/tools.rs` to confirm trait shape. Open `types.rs` for `ProposedToolCall`, `AgentRunContext`. Open `refs/stakpak/cli/src/commands/agent/run/tooling.rs` for a concrete impl.

CLAUDE CODE SECONDARY: Read `refs/claude-code/Tool.ts` for conceptual clarity on what "a Tool is".

IMPLEMENT in libs/agent-core/src/:
1. `tool.rs` — Tool trait
2. `executor.rs` — ToolExecutor trait (matches stakpak_arch.md section 8)
3. `registry.rs` — ToolRegistry; name → Tool dispatch
4. `errors.rs` — ToolError per stakpak_arch.md section 8 error taxonomy
5. `tests/` — trivial EchoTool; registration; dispatch by name; error propagation

CONSTITUTION CHECKS:
- Article I — foundation of agent discipline
- Article IV — failures are loud
- Article XIII rule 3 — no unwrap/expect/string-slice

Cite stakpak_arch.md section 8 file:line references in code comments.
<<<
```

---

## P-03 — First LLM call via stakai (with BYOK + 5-layer resolution)

**When to use:** After P-02.

**Reference sections:** `stakpak_arch.md section 10` (stakai LLM SDK), `section 15` (configuration types), `section 11` (RunOverrides merge order).

**Prompt:**

```
>>>
Wire up the first end-to-end LLM call with full BYOK support and the 5-layer model resolution chain mirroring Stakpak. Read stakpak_arch.md sections 10, 11, and 15 first.

The contract: model selection is a runtime decision, never compile-time. Customers BYOK from day one. Stage 1 ships with stakai's stock providers; custom providers added later via P-22.

IMPLEMENT in libs/ai/src/lib.rs:

1. `Client` struct that wraps stakai's `Inference`. Built from a resolved `ProviderConfig`.

2. Tier-aware API:
   `pub async fn complete(tier: Tier, prompt: &str) -> Result<String>`
   `pub async fn complete_with_tools(tier: Tier, prompt: &str, tools: Vec<ToolSpec>) -> Result<ToolCall>`
   where Tier is `enum Tier { Eco, Smart }`. Resolved to a concrete model via the active profile's `[tiers]` mapping.

3. The 5-layer resolution chain (priority lowest to highest):
   - Layer 1: `~/.terrashift/config.toml` `[profiles.default].model` and `[profiles.default.tiers]`
   - Layer 2: `--profile` CLI flag selects a different `[profiles.X]` block
   - Layer 3: `--model provider/id` CLI flag overrides for the entire session
   - Layer 4: `/model` slash command in TUI overrides from this turn onward (sends `OutputEvent::SwitchToModel`)
   - Layer 5: `RunOverrides { model, system_prompt, ... }` per-call (Stage 6+ API only)

4. The merge implemented in `libs/ai/src/resolver.rs`:
   ```
   caller.overrides.model
     > caller.body.model
       > AppState.active_model (from last checkpoint metadata)
         > AppState.default_model (from resolved profile)
           > catalog[0]  // safety net
   ```

5. Streaming via stakai's existing SSE; conversion to vendor shape happens fresh per request inside stakai's `convert.rs` (late-binding — model can change between turns, conversation history preserved verbatim because Vec<ChatMessage> is provider-neutral).

CONFIG SCHEMA — load via `toml` + `serde`. Define in libs/shared/src/config.rs:
```toml
[profiles.default]
model = "anthropic/claude-haiku-4-5"

  [profiles.default.tiers]
  eco = "anthropic/claude-haiku-4-5"
  smart = "anthropic/claude-opus-4-7"

  [profiles.default.providers.anthropic]
  type = "anthropic"
  api_key_env = "ANTHROPIC_API_KEY"

  [profiles.default.providers.openai]
  type = "openai"
  api_key_env = "OPENAI_API_KEY"
```

PROVIDER CATALOG REGISTRATION — at startup, `ProviderRegistry::register()` is called for each `[profiles.X.providers.Y]` block found in config. Auto-registration silently omits providers without resolvable credentials (per stakpak_arch.md libs/ai/src/registry/mod.rs:78 pattern). Users can't pick `openai/*` if no `[providers.openai]` block exists — fail loud at the resolver, not silently in the LLM call.

POLICY VS PREFERENCE SPLIT (mirroring Stakpak's convention but enforcing it more strictly):
- **Preference** (customer overrides allowed): model choice, API keys, endpoint URLs, downward budget cap, tracing opt-out
- **Policy** (operator-only, locked at profile level): tier→model mapping, Validator on the path, audit log on, max_turns cap, allowed providers list
- **Locked entirely** (no override anywhere): redaction layer, Article XIII rule 5, audit signing keys

Implement enforcement in resolver: when merging, policy fields from profile override caller-supplied values silently; preference fields take caller value.

CREDENTIAL HANDLING: Per Article V — the broker (libs/creds, P-10) handles BOTH cloud creds AND LLM provider keys. `api_key_env = "ANTHROPIC_API_KEY"` is resolved at LLM-call time, value zeroized after request. The LLM client never holds the raw key as an instance field; it asks the broker on each call.

DESCEND INTO SOURCE:
- `refs/stakpak/libs/ai/src/registry/mod.rs` — auto-registration pattern
- `refs/stakpak/libs/ai/src/client/builder.rs` — InferenceConfig construction
- `refs/stakpak/libs/server/src/routes.rs:545-583` — RunOverrides merge logic (the canonical reference)
- `refs/stakpak/cli/src/config/profile.rs` — profile/provider config types

TESTS:
- Mock the provider with stakai's test helpers
- Confirm 5-layer resolution: each layer beats lower-priority layers
- Confirm tier dispatch: same prompt with Tier::Eco and Tier::Smart goes to different concrete models
- Confirm late-binding: switching model mid-conversation preserves message history
- Confirm policy fields can't be overridden by caller
- Confirm preference fields can be overridden
- Confirm provider auto-omission: profile without OPENAI_API_KEY can't pick openai/* models

CONSTITUTION CHECKS:
- Article V (LLM provider keys treated as credentials, not config strings)
- Article X (every call gets a tracing span with model_id + provider as attributes)
- Article XII rule 3 (tier-aware routing)
- Article XIII rule 5 (don't bypass redaction; tier-aware redaction applies the same way regardless of which provider is selected)
- Article XIII rule 10 (don't store API keys in profiles outside ~/.terrashift/)

Cite stakpak_arch.md sections 10, 11, 15 in code comments at the resolver boundary.
<<<
```

---

## P-04 — Scanner (deterministic HCL parser)

**When to use:** First migration-pipeline component. After P-03.

**Reference sections:** None directly — Scanner is Terrashift-specific (cross-cloud migration domain).

**Prompt:**

```
>>>
Implement the Scanner. Reads a directory of `.tf` files and produces a typed `EstateInventory`.

This is a Terrashift-specific pipeline component (no direct Stakpak equivalent). Does NOT implement the Tool trait directly; called from the orchestrator.

In libs/engine/src/scanner/mod.rs:

1. Use `hcl-rs` to parse all `.tf` files in a directory tree
2. Extract: resources (type, name, attributes), modules, variables, outputs, providers
3. Produce `EstateInventory` — strongly-typed Rust struct (defined in libs/shared)
4. Detect issues: unsupported resource types (Stage 1 list), syntax errors, missing required attributes
5. Output to JSON for downstream stages and human-readable summary

NO LLM in this stage. Pure deterministic parsing.

CROSS-CUTTING: Read stakpak_arch.md section 28 (reversible file operations). Scanner is read-only; the eventual Generator will write, and it must follow the .backup/ pattern.

TESTS:
- Sample fixtures in libs/engine/tests/fixtures/
- Round-trip: parse → serialize → parse should be idempotent
- Edge cases: empty modules, nested modules, dynamic blocks (Stage 1 = error out cleanly per Article IV)

CONSTITUTION CHECKS:
- Article I (Scanner is deterministic, not an agent)
- Article IV (unsupported features fail loudly)
- Article XIII rule 3 (no unwrap; failures via Result)
<<<
```

---

## P-05 — Mapper (LLM with structured output, single call)

**When to use:** After P-04.

**Reference sections:** `stakpak_arch.md section 8` (agent loop), `section 20` (tool call lifecycle), `section 23` (context trimming with cache preservation).

**Prompt:**

```
>>>
Implement the Mapper. Takes an `EstateInventory` (from Scanner), produces a `MappingPlan` listing target resources for each source resource.

NOT AN AGENT. Single LLM call producing structured output, then deterministic dispatch. This is the pattern stakpak_arch.md section 40 calls "LLM with structured output" — distinct from a true agent that loops.

PRIMARY REFERENCE: stakpak_arch.md section 10 covers stakai's tool-call shape. We use a single tool call where the "tool" is `emit_mapping_plan` with a strict JSON schema.

IMPLEMENT in libs/engine/src/mapper/mod.rs:

1. Build a JSON schema for output structure using `schemars` derive
2. Construct the prompt: source provider, target provider, resource list, few-shot examples from libs/knowledge
3. Single LLM call via libs/ai's `complete_with_tools`
4. Validate response against schema; fail loudly per Article IV if non-conforming
5. Cache the result by input hash per Article XII rule 2

CROSS-CUTTING: Read stakpak_arch.md section 23 (context trimming with cache preservation). Mapper prompts must respect cache-stability invariant — once shipped, changes invalidate every cached mapping. Plan accordingly.

CONSTITUTION CHECKS:
- Article I (LLM-with-structured-output, not an agent — call out in PR description)
- Article III (output validated against schema)
- Article XII (cached, regression-gated)
- Article XIII rule 1 (don't bypass message conversion; output goes through validate before downstream)
<<<
```

---

## P-06 — Validator against live provider schemas

**When to use:** After P-05.

**Reference sections:** None — Article III's enforcement point, Terrashift-specific.

**Prompt:**

```
>>>
Implement the Validator. Takes a `MappingPlan` (from Mapper) and validates every target attribute against the live target-provider schema.

In libs/engine/src/validator/mod.rs:

1. Load schema from libs/knowledge (see P-07)
2. For each mapped resource: check resource type exists, every attribute exists, every required attribute is set, types match
3. Emit `ValidationReport`: passed, warnings, errors
4. Errors blocking; warnings surfaced but don't block

This is the single most important Article III enforcement point. If Mapper hallucinated an attribute, Validator catches it before HCL emit.

NO LLM. Pure schema lookup and comparison.

TESTS:
- Valid mapping passes
- Mapping with hallucinated attribute fails with clear error per Article IV
- Missing required attribute fails
- Type mismatch fails

CONSTITUTION CHECKS:
- Article III (every LLM-emitted attribute checked against schema)
- Article IV (validation errors are loud)
- Article XIII rule 3 (failures via Result, not panics)
<<<
```

---

## P-07 — Knowledge service (Stage 1: schema cache only)

**When to use:** Together with or just before P-06.

**Reference sections:** `stakpak_arch.md section 9` (api crate's SessionStorage trait — we adapt for our schema cache).

**Prompt:**

```
>>>
Implement the Knowledge service in libs/knowledge. Stage 1 scope: schema cache only (no RAG yet — Stage 3).

PRIMARY REFERENCE: stakpak_arch.md section 9 covers stakpak-api's SessionStorage trait pattern. We adapt the same shape for schema cache: trait with sync/async methods, libsql/sqlite default impl, optional remote impl later.

IMPLEMENT:

1. `pub trait SchemaStore` — trait with `fetch_schema(provider, version)`, `cache_schema(...)`, `list_versions(...)`. Mirror SessionStorage's shape from section 9.
2. `LocalSchemaStore` — sqlx-backed default impl using SQLite. Schema: `(provider, version, schema_json, fetched_at)` PK on (provider, version).
3. `pub async fn fetch_provider_schema(provider, version) -> Result<ProviderSchema>` — public API. Hits cache first; on miss, calls registry.terraform.io and populates cache.
4. Cache TTL: schemas pinned by version don't expire (Article VI); "latest" lookups expire after 24h.
5. `ProviderSchema` typed Rust struct: resources → attributes → type/required/description.

DESCEND INTO SOURCE: For SessionStorage pattern, read `refs/stakpak/libs/api/src/storage.rs`. Our trait is structurally the same but covers schemas instead of agent sessions.

TESTS:
- First fetch hits registry, populates cache
- Second fetch returns from cache (no network)
- Version-pinned fetch never expires

CONSTITUTION CHECKS:
- Article VI (schemas version-pinned per migration; no "latest" in production paths)
- Article X (every fetch logged via tracing)
<<<
```

---

## P-08 — Generator (deterministic HCL emit)

**When to use:** After P-06 and P-07.

**Reference sections:** `stakpak_arch.md section 28` (reversible file operations).

**Prompt:**

```
>>>
Implement the Generator. Takes a validated `MappingPlan`, produces target `.tf` files.

PRIMARY REFERENCE: stakpak_arch.md section 28 covers Stakpak's reversible file operations pattern — every file modification backed up to .backup/{uuid}/ before write. Generator MUST follow this pattern. Article V (security and reversibility).

IMPLEMENT in libs/engine/src/generator/mod.rs:

1. For each mapped resource, look up a template (deterministic; templates hard-coded for Stage 1, externalized to terrashift-mappings later)
2. Fill the template using attributes from the source resource
3. For attributes needing translation (e.g., AWS region → GCP region), use cached mapping from libs/knowledge
4. Emit HCL via hcl-rs's serialization
5. Write through backup-first wrapper: existing files copied to .backup/{run_id}/ before overwrite (per section 28)

NO LLM in happy path. Templates are deterministic. Only fall back to LLM if a template is missing AND the resource is in scope (rare in Stage 1; if it happens, log loudly per Article IV).

DESCEND INTO SOURCE: For reversible-file pattern, read `refs/stakpak/libs/agent-core/src/` and search for `.backup` references. Implementation uses move semantics: existing file moved to backup, new written; rollback is move-backup-back-to-original.

TESTS:
- Round-trip: emit → re-parse should be valid HCL
- Backup-first: existing files preserved before overwrite
- Template miss: falls back gracefully or fails loudly

CONSTITUTION CHECKS:
- Article I (deterministic-first)
- Article V (reversible operations — cite section 28 in code comments)
- Article XIII rule 5 (don't bypass redaction layer; if LLM invoked in fallback, redact first)
<<<
```

---

## P-09 — Executor (sandboxed terraform apply)

**When to use:** After P-08.

**Reference sections:** `stakpak_arch.md section 29` (Warden sandbox), `section 30` (shell command-level approvals).

**Prompt:**

```
>>>
Implement the Executor. Runs `terraform plan` and `terraform apply` against generated HCL, in sandboxed environment with command-level approval.

CRITICAL REFERENCES:
- stakpak_arch.md section 29 (Warden sandbox) — when warden.enabled = true, agent invocation re-exec'd inside Docker container. We adopt for terraform apply specifically.
- stakpak_arch.md section 30 (shell command-level approvals) — instead of approving `run_command` wholesale, tree-sitter-bash parser extracts each command in pipeline, applies most restrictive policy. We adopt for terraform commands. CRITICAL per Article XIII rule 6 — never approve terraform apply wholesale.

IMPLEMENT in libs/engine/src/executor/mod.rs:

1. Create isolated working dir under /tmp/terrashift-exec-{uuid}
2. Copy generated .tf files there
3. Run `terraform init`
4. Run `terraform plan -out=plan.tfplan`. Capture output.
5. Surface plan to user via approval flow (section 20 + section 30)
6. On confirm: run `terraform apply plan.tfplan`
7. Capture all output, exit codes, state changes
8. Stream tool output via tracing spans
9. Audit-log every action per Article V

DESCEND INTO SOURCE: Read `refs/stakpak/libs/shell-tool-approvals/src/` for tree-sitter-bash command parser. Pattern: parse command into syntax tree, walk it, apply scope::cmd::arg rule map. Adopt directly for terraform commands.

For Warden, read `refs/stakpak/cli/src/commands/warden.rs` and architecture in section 29.

CREDENTIALS: Use credential broker from libs/creds (P-10). For Stage 1, allow STS-token-based AWS credentials only.

TESTS:
- Use Terraform's mock provider
- Confirm sandboxing (executor cannot read outside working dir)
- Confirm plan shown before apply
- Confirm audit entries written
- No `unwrap()` per Article XIII rule 3

CONSTITUTION CHECKS:
- Article V (sandboxed apply, short-lived credentials, command-level approval)
- Article X (every action traced)
- Article XIII rule 6 (don't approve terraform apply wholesale; cite section 30)
- Article XIII rule 7 (don't write disk-bound secrets to bypass UID issues; if container UID issues arise, follow gosu pattern in section 29)
<<<
```

---

## P-10 — Credential broker

**When to use:** Together with P-09.

**Reference sections:** `stakpak_arch.md section 27` (secret detection / redaction), `section 9` (shared crate where secret types live).

**Prompt:**

```
>>>
Implement the credential broker in libs/creds. Single source of truth for any operation needing cloud credentials.

PRIMARY REFERENCE: stakpak_arch.md section 27 covers Stakpak's secret detection and redaction. Pattern: gitleaks rule set + entropy filter detect API keys, tokens, certificates. Detected secrets replaced in-place with stable tokens [REDACTED_SECRET:rule:6char_id]; redaction map persisted at .stakpak/session/secrets.json.

We adopt this directly with terrashift-prefix tokens.

IMPLEMENT:

1. `pub trait CredentialBroker` — async trait with `fetch_aws(role)`, `fetch_gcp(account)`, `fetch_azure(subscription)`. Each returns short-lived credentials.
2. `AwsBroker` — uses STS AssumeRole; never holds long-term keys
3. `GcpBroker` — uses Application Default Credentials with workload identity federation when available
4. `AzureBroker` — uses managed identity or service principal with short-lived tokens
5. Secret substitution: LLM works with references like `{{secret:aws-prod-deploy}}`; broker resolves at execution time only (per section 27)
6. Pre-prompt scrubber: gitleaks-style patterns + entropy-based detection of high-entropy strings; replaces with tokens before LLM context (per section 27)
7. Every fetch audit-logged with timestamp + invoking operation
8. Credentials zeroized in memory after use using `zeroize` crate

DESCEND INTO SOURCE: For Stakpak's secret manager, read `refs/stakpak/libs/shared/src/secrets/` and `secret_manager.rs`. Redaction patterns and entropy thresholds directly applicable.

PRIVACY MODE: Per section 27, --privacy-mode adds IP addresses, AWS account IDs, PII patterns. We adopt and add cross-cloud-specific patterns (GCP project IDs, Azure subscription GUIDs).

TESTS:
- Mock STS, confirm short-lived tokens fetched and dropped
- Confirm raw credentials never appear in any returned ToolCall struct
- Confirm audit log entries
- Confirm zeroize called on credential drop
- Confirm secret substitution roundtrips correctly

CONSTITUTION CHECKS:
- Article V — heart of it. Cite in all tests and PR description.
- Article XIII rule 5 (don't bypass redaction layer)
- Article XIII rule 7 (don't write disk-bound secrets)
- Article XIII rule 10 (don't store API keys outside ~/.terrashift/)
<<<
```

---

## P-11 — Audit log

**When to use:** Together with P-09 and P-10.

**Reference sections:** `stakpak_arch.md section 22` (checkpoint and resume — same lifecycle hook points).

**Prompt:**

```
>>>
Implement the audit log in libs/audit. Every meaningful operation in Terrashift writes an entry. Audit log is signed and append-only.

This extends Stakpak's pattern (they have tracing for observability per stakpak_arch.md section 32) into compliance-grade auditing for cross-cloud migrations. Compliance reviewers need a tamper-evident log; Stakpak's tracing isn't structured for that.

IMPLEMENT in libs/audit/src/:

1. Schema (entry.rs) — split base + variant:

   struct AuditEntry {
       // Base fields — every entry
       id: Uuid,
       timestamp: DateTime<Utc>,
       run_id: Uuid,                  // ties entry to a migration run
       actor: Actor,                  // User { id } | System | Agent { name }
       operation: String,             // e.g. "tool.execute", "phase.transition"
       outcome: Outcome,              // Ok | Err { kind, message }
       payload: AuditPayload,         // variant — see below

       // Hash chain
       prev_hash: [u8; 32],
       content_hash: [u8; 32],
       signature: [u8; 64],           // Ed25519
   }

   enum AuditPayload {
       LlmCall {
           provider: String,            // "anthropic" / "openai" / "vodafone-internal"
           model_id: String,            // "claude-opus-4-7"
           provider_endpoint: String,   // resolved endpoint URL (no api_key)
           tier: Tier,                  // Eco | Smart
           input_tokens: u32,
           output_tokens: u32,
           cache_read_tokens: u32,
           cache_write_tokens: u32,
           cost_usd_micros: u64,        // 1e-6 USD precision; computed at log time from price-list
       },
       ToolExecution {
           tool_name: String,
           cred_ref: Option<String>,    // "{{secret:aws-prod}}" — never the resolved value
           target: String,              // e.g. "aws_vpc.main"
           duration_ms: u32,
       },
       PhaseTransition {
           from: Phase,
           to: Phase,
           checkpoint_id: Option<Uuid>,
       },
       CredentialResolution {
           cred_ref: String,            // "{{secret:aws-prod}}"
           resolution_method: String,   // "sts_assume_role" / "gcp_adc" / "azure_managed_identity" / "env_lookup"
           expires_at: DateTime<Utc>,
       },
       FileOperation {
           path: PathBuf,
           kind: FileOpKind,            // Create | Modify | Delete
           backup_path: Option<PathBuf>,
       },
   }

2. Hash chain: each entry's prev_hash is SHA-256 of previous entry's content_hash. content_hash is SHA-256 of the canonical JSON serialisation excluding signature + content_hash + prev_hash fields.

3. Signature: Ed25519 over content_hash. Per-migration key generated at run start, public key persisted in run metadata so verification works after the fact.

4. Storage: SQLite append-only table (libs/audit/src/store.rs). Schema:
   audit_entries(id, run_id, timestamp, payload_json, prev_hash, content_hash, signature) PK on id, INDEX on (run_id, timestamp).

5. Public API:
   pub fn append(entry: AuditEntry) -> Result<()>     // only mutation
   pub fn verify_chain(run_id: Uuid) -> Result<()>     // validates entire chain on demand
   pub fn export(run_id: Uuid) -> Result<Vec<AuditEntry>>  // for compliance review
   pub fn query(filter: AuditFilter) -> Result<Vec<AuditEntry>>  // for dashboards

REDACTION INVARIANT (Article XIII rule 5): the audit log writer is the LAST place secrets could leak. Before serialising payload_json to disk:
- Run pre-prompt scrubber over every string field
- Confirm cred_ref values match {{secret:...}} pattern, not raw values
- Reject and panic if any field contains gitleaks-detected patterns (loud failure per Article IV)

INTEGRATION POINTS (where append() is called from):
- libs/ai's Client: emit LlmCall payload after every successful inference (input_tokens, output_tokens, cost computed from current price list)
- libs/agent-core's after_tool_execution hook: emit ToolExecution payload
- libs/engine's phase coordinator: emit PhaseTransition payload at every phase boundary (see LLD-2)
- libs/creds's CredentialBroker::fetch_*: emit CredentialResolution payload
- libs/engine/src/generator's backup-first wrapper: emit FileOperation payload

CROSS-CUTTING: Reference stakpak_arch.md section 22 (checkpoint and resume). Audit log entries written at same lifecycle points as checkpoints — but different purposes. Both run through hooks. Audit chain is INDEPENDENT of checkpoint chain — losing a checkpoint must not break audit verification.

DESCEND INTO SOURCE: For checkpoint patterns audit log emulates, see `refs/stakpak/libs/server/src/checkpoint_store.rs`. The hash-chained structure of CheckpointEnvelope is the closest analog.

TESTS:
- Append + verify round-trip per payload variant
- Tampering with any field breaks chain verification (with the specific field flagged)
- Tampering with signature is detected separately from payload tampering
- Performance: appends are O(1), verify_chain is O(n) but acceptable up to 10K entries
- Redaction: an entry containing a raw API-key-like string is REJECTED, not stored
- Multi-run isolation: appending to run_id A doesn't affect chain verification of run_id B
- Cost computation: LlmCall payload's cost_usd_micros matches input_tokens × input_price + output_tokens × output_price within rounding tolerance

CONSTITUTION CHECKS:
- Article V (audit) — heart of it; cite in PR
- Article V audit-records-the-model invariant: every LlmCall payload has provider, model_id, provider_endpoint
- Article IX (data governance — audit logs are user property, never auto-deleted)
- Article XIII rule 3 (no unwrap; verification failures via Result)
- Article XIII rule 5 (don't bypass redaction; the writer is the last line of defence)
<<<
```

---

## P-12 — Eval framework

**When to use:** Stage 1, before any major refactor.

**Reference sections:** `stakpak_arch.md section 32` (CI matrix).

**Prompt:**

```
>>>
Implement eval framework in libs/eval. This protects from regressions and proves the system works.

Pattern is Terrashift-specific (Stakpak doesn't have golden-migrations because their domain doesn't require it), but CI integration follows Stakpak's pattern from stakpak_arch.md section 32.

IMPLEMENT:

1. `struct GoldenMigration { source_tf, expected_target_tf, expected_audit_summary }`
2. `pub async fn run_eval(migration: &GoldenMigration) -> EvalResult` — runs full pipeline, compares output
3. `EvalResult` includes: pass/fail, diff against expected, token cost, wall-clock time
4. Suite runner: runs all golden migrations, produces summary report
5. CI integration: regression on token cost (>30%) or eval pass rate (<100%) blocks merge

Golden migrations live in terrashift-evals (private repo). Stage 1 starts with 5–10 hand-curated migrations covering GCP→AWS scope.

CROSS-CUTTING: Read stakpak_arch.md section 32 (CI matrix). Eval suite is one of gated CI checks. Pattern after their feature-gated test categories.

CONSTITUTION CHECKS:
- Article XII (token-cost regression gate; see also Article XIII rule 2 — non-monotonic cache breaks regression detection)
- Article III (evals are source of truth for AI safety)
<<<
```

---

## P-13 — Single-binary release pipeline

**When to use:** End of Stage 1 setup.

**Reference sections:** `stakpak_arch.md section 33` (release pipeline), `section 34` (release.sh), `section 36` (Docker image).

**Prompt:**

```
>>>
Set up cross-platform release pipeline. End state: `cargo build --release` produces single binary; GitHub Actions produces signed binaries for Linux x86_64 (musl, fully static) and macOS arm64.

PRIMARY REFERENCE: stakpak_arch.md section 33 covers 5-target build matrix. We pattern after them with Terrashift-specific targets.

CROSS-CUTTING:
- section 34 covers release.sh workflow for version bumping
- section 35 covers cliff.toml for changelog generation
- section 36 covers Docker image (lean image w/ aqua-pinned tool versions)

IMPLEMENT in .github/workflows/release.yml:

1. Trigger on tag push (`v*`)
2. Matrix build: Linux x86_64 (musl, static), macOS arm64 (universal), Linux arm64 (musl), Windows x64 (optional Stage 1)
3. Build with `cargo build --release --target <triple>`
4. Strip binaries
5. Sign macOS with code signing (defer to Stage 6 if cert not yet acquired)
6. Generate SHA256 sums
7. Create GitHub Release with binaries, sums, CHANGELOG.md excerpt (generated by git-cliff per section 35)
8. Update Homebrew tap (defer to Stage 6 if tap not yet acquired)

For static linking: rustls-tls everywhere (chosen in P-01). No OpenSSL.

DESCEND INTO SOURCE: Copy `refs/stakpak/.github/workflows/build-and-release.yml` and adapt.

TESTS:
- Local: `cargo build --release` produces binary that runs on fresh container
- CI: test tag triggers full build; downloads run on fresh OS images

CONSTITUTION CHECKS:
- Article VII (CI)
- Article VIII (Stage 1 exit gate requires single-binary distribution)
<<<
```

---

## P-14 — Slash commands in TUI

**When to use:** After Stage 1 core is functional.

**Reference sections:** `stakpak_arch.md section 16` (TUI), `section 22` (checkpoint and resume).

**Prompt:**

```
>>>
Implement slash commands in TUI: /help, /plan, /cost, /migrate, /rollback, /audit, /compact, /checkpoint.

PRIMARY REFERENCE: stakpak_arch.md section 16 covers stakpak-tui's structure — AppState, services/handlers/, dual-channel mpsc contract (InputEvent / OutputEvent). Slash command pattern is part of this.

CLAUDE CODE SECONDARY: Per-command-file pattern is cleaner in Claude Code's commands/ directory. Read `refs/claude-code/commands/` and adopt: each command is a small file with metadata header and handler.

IMPLEMENT in tui/src/commands/:

1. `trait SlashCommand` — name, description, handler
2. Registry mapping `/name` to handler (HashMap, populated at TUI init)
3. Each command is a small file: help.rs, plan.rs, cost.rs, migrate.rs, rollback.rs, audit.rs, compact.rs, checkpoint.rs
4. /help lists all available commands with descriptions
5. /compact triggers conversation compaction (maps to stakpak_arch.md section 8 compaction engine)
6. /checkpoint saves current state for resume (maps to section 22)

CROSS-CUTTING: Read stakpak_arch.md section 22 (checkpoint and resume) for /checkpoint behaviour. Checkpoint envelope is V1 schema; /checkpoint produces one with our run_id and message history.

TESTS:
- Each command has unit test for handler
- Registry dispatch by name
- Unknown command produces friendly error per Article IV

CONSTITUTION CHECKS:
- Article II (Claude Code's slash-command pattern is one of few things we directly borrow conceptually)
- Article XIII rule 8 (don't add new InputEvent/OutputEvent variants without updating is_backend_event() — slash commands send these events)
<<<
```

---

## P-15 — Eval suite expansion

**When to use:** Throughout Stage 1, as new pipeline components ship.

**Prompt:**

```
>>>
Expand golden migrations eval suite. Goal: 10 migrations covering Stage 1's GCP→AWS scope.

For each new pipeline component (Mapper, Generator, etc.), add at least one golden migration that exercises it.

For each, document in terrashift-evals/README.md:
- Source TF
- Expected target TF
- Expected audit log summary (per Article V)
- Expected token cost ceiling (per Article XII rule 1)
- Which constitution articles this eval validates

Update CI to run full suite on every PR (Article XII rule 4 + Article XIII rule 2 — cache stability matters).

Real-world migration sources to draw from:
- PratikMahajan/AWS-to-AZURE-Infrastructure-Migration (full TF for both clouds, 20+ services)
- Ben Foster's GCP migration write-up (8 documented translation differences)
- terraformer (CLI tool generating TF from existing infrastructure — use to build paired examples at scale)

CONSTITUTION CHECKS:
- Article III (evals are source of truth)
- Article XII rule 4 (regression gate)
- Article XIII rule 2 (non-monotonic trim boundaries break regression detection)
<<<
```

---

## P-16 — Stage 1 exit gate review

**When to use:** End of Stage 1.

**Prompt:**

```
>>>
We are at Stage 1 exit gate. Review system against gate criteria from terrashift_plan.md section 17.

For each criterion: pass / fail / partial. For partials, what's missing.

1. Demo passes for 15-resource sample (run end-to-end)
2. Audit log signed (verify chain integrity per Article V + libs/audit)
3. Re-running produces identical output (Article VI version-pinning + Article VIII reproducibility)
4. Token cost <$15 for sample migration (Article XII)
5. All 13 constitution articles documented and at least one PR has cited each (Article II + Article XIII discipline)
6. Single-binary distribution works (Article XIII rule 7 — binary doesn't depend on host's library state)

If all pass: produce Stage 1 retrospective document highlighting what worked, didn't, what to change for Stage 2.

If any fail: produce remediation plan with specific tickets.

Do not write production code. Review only.
<<<
```

---

## P-22 — Adding a custom LLM provider

**When to use:** Stage 2+ when a customer needs a provider stakai doesn't ship (corporate Anthropic proxy with custom auth, internal Vodafone gateway, on-premise Bedrock, custom-fine-tuned model on local Ollama, etc.).

**Reference sections:** `stakpak_arch.md section 10` (stakai SDK), `section 39` (LLM-backend seam), `section 41 Phase 3` (canonical "replace the LLM provider" mirror playbook), `section 38` (the `feat/add-minimax-provider` reference branch).

**Prompt:**

```
>>>
Add `<provider-name>` as a custom LLM provider in libs/ai/src/providers/<provider-name>/. The canonical reference for this work is the `feat/add-minimax-provider` branch in refs/stakpak/ (see stakpak_arch.md section 38) — read its diff before starting.

PRIMARY REFERENCE: stakpak_arch.md section 41 Phase 3 lists exactly the files to add and touch:

FILES TO ADD:
- libs/ai/src/providers/<provider-name>/convert.rs   — convert stakai types to/from this provider's wire format
- libs/ai/src/providers/<provider-name>/mod.rs       — module entry, re-exports
- libs/ai/src/providers/<provider-name>/provider.rs  — `impl Provider for <Name>Provider` (the 4 required methods)
- libs/ai/src/providers/<provider-name>/stream.rs    — SSE streaming parsing (provider-specific wire format)
- libs/ai/src/providers/<provider-name>/types.rs     — provider-specific request/response types

FILES TO TOUCH:
- libs/ai/src/client/builder.rs       — add config branch
- libs/ai/src/client/config.rs        — add InferenceConfig variant
- libs/ai/src/provider/dispatcher.rs  — add dispatch case
- libs/ai/src/registry/mod.rs         — add to provider registry
- libs/ai/src/providers/mod.rs        — re-export the new module
- libs/shared/src/models/llm.rs       — add ProviderConfig variant (Anthropic-style? OpenAI-style? Custom?)
- libs/shared/src/models/stakai_adapter.rs  — add adapter variant
- cli/src/commands/auth/login.rs      — add --provider <name> path (interactive setup)
- cli/src/config/app.rs               — add credential resolution branch
- cli/src/config/profile.rs           — add validation for [profiles.X.providers.<name>]
- cli/src/onboarding/config_templates.rs  — add wizard template

THE 4 PROVIDER TRAIT METHODS:
1. `async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse>` — blocking round-trip
2. `async fn stream(&self, request: GenerateRequest) -> Result<GenerateStream>` — futures::Stream of StreamEvent
3. `fn list_models(&self) -> Vec<Model>` — for provider catalog and model picker
4. `fn provider_key(&self) -> &str` — the lookup key in `[profiles.X.providers.<key>]`

WIRE FORMAT WORK:
The provider-specific complexity lives in convert.rs and stream.rs. Read the provider's docs for:
- Request format (Anthropic Messages API? OpenAI Chat Completions? Custom?)
- Streaming format (SSE event types, content delta encoding)
- Tool-call shape (function calls? tool_use blocks? something else?)
- Auth method (Bearer? API key header? mTLS? OAuth2 with refresh?)

Most "OpenAI-compatible" providers can reuse libs/ai/src/providers/openai/ with a custom api_endpoint — start there before writing a new provider from scratch. If the wire format truly differs, follow the MiniMax pattern.

CONFIG SHAPE — what users add to ~/.terrashift/config.toml:
```toml
[profiles.default.providers.<name>]
type = "<name>"           # matches provider_key()
api_endpoint = "https://..."
api_key_env = "<NAME>_API_KEY"
# Plus any provider-specific fields
```

REDACTION: Per Article V and Article XIII rule 5, provider responses pass through libs/mcp/proxy redaction before reaching the LLM context. Custom providers don't bypass this — the proxy boundary is provider-agnostic.

POLICY: If the operator wants to restrict customers to specific providers, they set `[profiles.default.allowed_providers] = ["anthropic", "openai"]` and the resolver rejects models from other providers at parse time.

TESTS:
- Integration test in libs/ai/tests/integration/<provider-name>.rs (mirrors libs/ai/tests/integration/minimax.rs from the reference branch)
- Confirm provider auto-registers when [providers.<name>] block is present
- Confirm provider auto-omits when block is absent (silent omission, not failure — fail loud only when user tries to USE it)
- Confirm streaming produces correct StreamEvent sequence
- Confirm tool-call round-trip works
- Confirm credentials are zeroized after request (via Drop impl)

CONSTITUTION CHECKS:
- Article II (citing the Stakpak feat/add-minimax-provider branch as canonical reference)
- Article V (provider keys treated as credentials)
- Article XII (the new provider's per-tier cost ceiling needs to be added to the eval baseline)
- Article XIII rule 5 (redaction boundary unchanged)
- Article XIII rule 10 (no API keys outside ~/.terrashift/)

Output: working provider impl, all tests green, README entry under "Supported providers" updated.
<<<
```

---

## Other prompts (briefer, used as needed)

### P-17 — Adding a new tool

```
Add a new tool: <tool-name>. Implements Tool trait in libs/agent-core. Schema in schemars. Tests cover happy path + error paths. Cite stakpak_arch.md section 8 and which constitution articles apply.
```

### P-18 — Adding a new resource type to Stage 1 mapping corpus

```
Add `<source-resource>` ↔ `<target-resource>` mapping. Update terrashift-mappings repo. Add golden migration. Update Validator's known-types list (libs/engine/src/validator/known_types.rs).
```

### P-19 — Investigating a token cost regression

```
Token cost increased by <X>% on <PR-number>. Investigate which prompts contributed. Apply Article XII rules in priority:
1. Cache hit rate (and Article XIII rule 2 — verify trim boundaries are still monotonic; non-monotonic boundaries kill cache hits)
2. Tier routing decisions
3. Context window sizes
4. Whether pure-code path was bypassed (Article I — deterministic-first)
```

### P-20 — Translating a Claude Code pattern to Rust

```
Read Claude Code's <file>.ts. Translate the <pattern> to Rust idioms. Use stakai types where they exist; otherwise build minimal types in libs/agent-core. Cite Article II and the relevant stakpak_arch.md section if pattern overlaps with Stakpak's approach.
```

### P-21 — Translating a Stakpak pattern via the architecture document

```
Read stakpak_arch.md section<section>. The pattern documented there is what we want to mirror. Implement equivalent in Terrashift's libs/<crate>. Note any deviations from Stakpak's approach and document why in code comments. If the doc references a specific file:line in stakpak source, descend to that file to verify; if doc and code disagree, follow the code and note disagreement.
```

---

## Conventions

**Every prompt should:**
1. State which `stakpak_arch.md` section to read first (where applicable)
2. Specify file paths explicitly
3. List which constitution articles apply (including Article XIII rules where relevant)
4. Require tests
5. End with "do not write X" boundary if relevant

**Every PR should:**
1. Cite constitution articles touched
2. Cite `stakpak_arch.md` sections referenced (compact: `stakpak_arch.md sectionN`)
3. Cite Claude Code patterns adopted, if any
4. Include eval framework results
5. Stay under 500 lines preferred

This file is the source of truth for how we direct Opus 4.7. Updates are PRs that cite Article XI.
