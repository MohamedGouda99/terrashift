# Spec — P-02: ToolExecutor + AgentHook + supporting types

**Status:** in progress
**Branch:** main (single-author session; Spec Kit discipline preserved in artifacts)
**P-NN:** P-02 (terrashift_prompts.md lines 162-209)
**TERRASHIFT_MAPPING.md:** §A row 2 (ToolExecutor seam), row 3 (AgentHook seam)
**Constitution:** Article I (foundation of agent discipline), IV (failures loud), XIII rule 3

## Goal

Implement the foundation that every Terrashift tool will use:
`ToolExecutor` trait, `AgentHook` trait, `ToolExecutionResult` enum,
`ProposedToolCall`, `AgentRunContext`, `ToolDecision`, `AgentError`.

These are the kernel-level seams from `stakpak_arch.md` §8 (THE CANONICAL
AGENT LOOP). Per TERRASHIFT_MAPPING.md §A: **mirror Stakpak verbatim** — same
trait signatures, same enum variants, same supporting types. Stakpak is the
primary reference; deviations require RFC.

## Scope (Stage 1 P-02)

**In scope:**
- `libs/agent-core/src/tools.rs` — `ToolExecutor` trait + `ToolExecutionResult` enum
- `libs/agent-core/src/hooks.rs` — `AgentHook` trait (5 lifecycle methods, all default no-op)
- `libs/agent-core/src/error.rs` — `AgentError` enum (subset — only variants needed by P-02)
- `libs/agent-core/src/types.rs` — `AgentRunContext`, `ProposedToolCall`, `ToolDecision`
- `libs/agent-core/src/registry.rs` — `ToolRegistry` (HashMap-backed convenience that impls `ToolExecutor`; **Terrashift addition**, no Stakpak counterpart)
- `libs/agent-core/src/lib.rs` — re-exports
- `libs/agent-core/tests/echo_executor_test.rs` — `EchoExecutor` integration test
- Add `stakai` to agent-core's Cargo.toml (needed for `Message`, `Model` in hooks)

**Out of scope (deferred to later P-NN):**
- `agent.rs` / `run_agent` (Stage 2 — full agent loop)
- `approval.rs` / `ApprovalStateMachine` (P-09 — needed for sandboxed Executor)
- `compaction.rs`, `context.rs`, `budget_context.rs` (Stage 2 — context overflow)
- `checkpoint.rs` (P-14 — slash commands `/checkpoint`)
- `retry.rs`, `stream.rs` (Stage 2 — agent loop machinery)

The deferred files don't exist yet. `error.rs` will start minimal and grow as
those files arrive (each adds its own variant).

## Success criteria

- All 6 source files compile with `cargo check`
- `cargo clippy --all-targets -- -D warnings` passes (no `unwrap`/`expect`/`&s[..n]`)
- `cargo fmt -- --check` passes
- `cargo test -p terrashift-agent-core` passes 4 cases:
  - EchoExecutor returns input as Completed{result, is_error: false}
  - EchoExecutor honors CancellationToken (returns Cancelled)
  - ToolRegistry dispatches by name to registered executor
  - ToolRegistry returns ToolNotFound error for unknown name
- `lib.rs` re-exports match Stakpak's pattern (subset)

## Reference

- `refs/stakpak/libs/agent-core/src/tools.rs` (verbatim)
- `refs/stakpak/libs/agent-core/src/hooks.rs` (verbatim)
- `refs/stakpak/libs/agent-core/src/types.rs` (subset — only AgentRunContext, ProposedToolCall, ToolDecision needed for P-02)
- `refs/stakpak/libs/agent-core/src/error.rs` (subset — only ToolExecution + Hook + Cancelled variants needed)
- `stakpak_arch.md` §8 (kernel description), §20 (tool call lifecycle)
