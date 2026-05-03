# Feature Specification: Agent Loop Kernel (S9)

**Feature Branch**: `015-agent-loop-kernel`
**Created**: 2026-05-03
**Status**: Draft
**Input**: SESSION_PLAN.md S9 — "Agent loop kernel: run_agent, ApprovalStateMachine, retry, stream — completes libs/agent-core"

## Context

Stage 2 introduces two agents (Recovery in S10, Cost Optimizer in S11). Both need a shared kernel: an async loop that calls the LLM, dispatches proposed tool calls through an approval gate, executes accepted tool calls via the existing `ToolExecutor`, fires `AgentHook` lifecycle events, and retries on transient errors. Stage 1 ships zero agents (Article I), so this kernel is **structurally inert** until S10's Recovery agent activates it.

Pattern source: `refs/stakpak/libs/agent-core/src/{agent,approval,retry,stream}.rs` (1754 lines combined). We narrow for Stage 2 by skipping `stream.rs` entirely (no Stage 2 consumer streams) and trimming interactive steering from `agent.rs` (S13 adds it back when detached mode lands).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Run loop dispatches LLM-proposed tools (Priority: P1)

The kernel's primary job: take an initial prompt, ask the LLM, get back a list of proposed tool calls, run each through the approval policy, execute the accepted ones via `ToolExecutor`, feed the results back to the LLM, repeat until the LLM produces a final answer or `max_turns` is reached.

**Why this priority**: This is the entire reason the kernel exists. S10 Recovery agent's bounded-ReAct loop and S11 Cost Optimizer's plan-refinement loop both use this exact path. Without it, neither S10 nor S11 can ship.

**Independent Test**: Stub `LlmClient` returns scripted responses; stub `ToolExecutor` echoes its input; assert the kernel runs the expected number of turns and returns the final answer text.

**Acceptance Scenarios**:
1. **Given** an LLM that proposes 1 tool call on turn 1 and a final answer on turn 2, **When** `run_agent` is invoked, **Then** the loop completes in 2 turns with the final answer text in `AgentLoopResult::final_text`.
2. **Given** an LLM that always proposes a tool call, **When** `max_turns = 3` is configured, **Then** the loop terminates with `AgentLoopResult::reason = MaxTurnsReached` after 3 turns.
3. **Given** a tool call rejected by the approval policy, **When** the loop runs, **Then** the rejection is fed back to the LLM as a tool result with status "rejected" and the loop continues.

### User Story 2 — Approval gate enforces tool-call ordering (Priority: P1)

Tool calls must be dispatched in the order the LLM proposed them. If the LLM proposes 3 calls in turn N, the kernel resolves call 0 → executes → resolves call 1 → executes → resolves call 2 → executes, even if user decisions arrive out of order. This prevents a "later call resolved first" race that could corrupt LLM context.

**Why this priority**: Article XIII rule 9 (no duplicate tool_call_id) + ordering invariant from `stakpak_arch.md §8`. Violation could deliver tool results in an order the LLM didn't expect, producing degenerate outputs.

**Independent Test**: Resolve calls 1+2 first (out of order), then resolve call 0; assert `next_ready()` returns call 0 first, then 1, then 2.

**Acceptance Scenarios**:
1. **Given** 3 tool calls and the user resolves call 2 before call 0, **When** the kernel polls `next_ready()`, **Then** `None` is returned until call 0 is also resolved.
2. **Given** all 3 calls resolved out of order, **When** the kernel drains via repeated `next_ready()`, **Then** the dispatched order matches the LLM's proposed order.

### User Story 3 — Retry on transient LLM errors (Priority: P1)

When the LLM call fails with a retryable error (rate limit, timeout, 5xx), the kernel waits per the retry policy (header-driven if available, exponential backoff otherwise) and retries up to `max_attempts`.

**Why this priority**: Without retry, a single rate-limit response kills an entire migration. Stakpak handles this via retry-after header parsing + exponential fallback. Stage 2 consumers (Recovery agent in particular) explicitly want this on the LLM call boundary.

**Independent Test**: Stub LLM that fails the first 2 attempts then succeeds; assert the kernel sleeps the expected delays and ultimately returns success.

**Acceptance Scenarios**:
1. **Given** an LLM that returns 429 with `retry-after: 2` then succeeds, **When** the kernel runs, **Then** the kernel sleeps ~2s and the call succeeds on attempt 2.
2. **Given** an LLM that always fails, **When** `max_attempts = 4` is configured, **Then** the kernel returns `AgentError::LlmRetryExhausted` after 4 attempts.

### User Story 4 — Hooks fire at all 5 lifecycle points (Priority: P2)

The `AgentHook` trait already has 5 methods (P-02). The kernel must call them at the right points so `libs/audit` (`after_tool_execution`) and `libs/creds` (`before_tool_execution`) can do their cross-cutting work.

**Why this priority**: P2 because audit hook coverage is what gives us Article V auditability for agent runs. The kernel itself works without hooks; this is the wiring that makes the agent's work visible.

**Independent Test**: A test hook that pushes the lifecycle phase name into a `Vec`; assert the vec contains the expected sequence after one turn.

**Acceptance Scenarios**:
1. **Given** a registered `AgentHook`, **When** `run_agent` runs one turn with one tool call, **Then** the hook receives `before_run`, `before_llm_call`, `after_llm_call`, `before_tool_execution`, `after_tool_execution`, `after_run` in that order.

### Edge Cases

- LLM returns a tool call with an `id` that duplicates a prior call's id → kernel returns `AgentError::DuplicateToolCallId` (Article XIII rule 9).
- `max_turns = 0` → kernel returns `AgentError::InvalidConfig`.
- Approval policy rejects every tool call and the LLM keeps proposing → kernel hits `max_turns` and returns the LLM's last text content (or empty if none).
- Compaction trigger at threshold → Stage 2 uses `PassthroughCompactionEngine`, so the trigger fires but no actual compaction happens (seam exists; impl is no-op).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The kernel MUST expose a single async entry point `run_agent(config, ctx, executor, hooks, llm, reducer, compactor) -> Result<AgentLoopResult, AgentError>`.
- **FR-002**: The kernel MUST drive the approval state machine in proposed-order: `next_ready()` only returns calls in the order the LLM emitted them, regardless of user resolution order.
- **FR-003**: The kernel MUST retry transient LLM errors per `RetryConfig` (initial backoff, multiplier, max attempts, header-driven override).
- **FR-004**: The kernel MUST fire all 5 `AgentHook` methods at their canonical points and propagate hook errors as `AgentError::HookFailure { phase, source }`.
- **FR-005**: The kernel MUST run `ContextReducer::reduce` on the message thread before every LLM call (`stakpak_arch.md §8 invariant`).
- **FR-006**: The kernel MUST run `CompactionEngine::compact` when the message thread crosses the configured threshold (Stage 2 = passthrough; seam exercised but no-op).
- **FR-007**: The kernel MUST reject duplicate `tool_call_id` within a turn (`AgentError::DuplicateToolCallId`).
- **FR-008**: The kernel MUST terminate when the LLM returns no tool calls (final answer reached) OR `max_turns` is exceeded (loud error).
- **FR-009**: When the approval state machine is `is_waiting_for_user()`, the kernel MUST return `Ok(AgentLoopResult { reason: WaitingForApproval, ... })` — interactive steering arrives in S13.
- **FR-010**: The kernel MUST NOT support streaming output in Stage 2 (`stream.rs` is deliberately deferred to S5+ when consumers stream).

### Key Entities

- **`AgentLoopConfig`** — bounded `max_turns`, `RetryConfig`, `ToolApprovalPolicy`, `compaction_threshold_tokens`. Constructed by the consumer (S10/S11), passed to `run_agent`.
- **`AgentLoopResult`** — terminal state: final text, final messages, terminal `reason` enum (FinalAnswerReached / MaxTurnsReached / WaitingForApproval / Aborted), token counts.
- **`ApprovalStateMachine`** — verbatim port from Stakpak. Stateful per turn; resets each turn.
- **`RetryConfig`** — Stakpak's shape: `max_attempts`, `initial_backoff_ms`, `max_backoff_ms`, `multiplier`.
- **`ToolApprovalPolicy`** — None / All / Custom { rules: HashMap<String, ToolApprovalAction>, default }.
- **`AgentCommand`** — Stage-2 narrows to `ResolveTool { tool_call_id, decision }` + `ResolveTools { decisions: HashMap }`. `Steer`, `FollowUp`, `SwitchModel`, `Abort` are deferred to S13.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All 4 user stories have at least one passing test in `libs/agent-core/tests/`.
- **SC-002**: `cargo test -p terrashift-agent-core` passes with ≥10 new tests added (P-02's test count + S9's tests).
- **SC-003**: `cargo clippy -p terrashift-agent-core --all-targets -- -D warnings` passes — Article XIII rule 3 enforcement on the kernel's hot path.
- **SC-004**: Workspace `cargo check --workspace` and `cargo test --workspace` remain green after this commit (no regression to S1-S8 tests).
- **SC-005**: Token cost regression eval: 0% delta vs baseline (kernel doesn't make LLM calls in Stage 1; baseline is all-zero per R6).

## Assumptions

- Streaming is deferred to S5+ (no Stage 2 consumer needs streaming).
- Interactive steering (mid-run model switch, follow-up prompt, abort) is deferred to S13 (detached mode).
- The kernel is consumer-driven: S10 Recovery agent will be the first concrete consumer; until then `run_agent` is unused production-path code (Article XII rule 1 acceptable: it's a structural seam, not dead code).
- Hooks come from existing `libs/audit/src/hooks.rs::AuditWriterHook` and `libs/creds/src/hooks.rs` (when S9-pre-req additions land); the kernel doesn't add new hook impls.
- The LLM client is the existing `libs/ai/src/client.rs::LlmClient` trait (Stage 2 may need to widen it for streaming + tool-call propagation; this spec narrows to `complete()` for now and surfaces the widening as an S10 prerequisite).
