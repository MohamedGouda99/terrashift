# Feature Specification: Recovery Agent (S10)

**Feature Branch**: `016-recovery-agent`
**Created**: 2026-05-03
**Status**: Draft
**Input**: SESSION_PLAN.md S10 — "Recovery agent: libs/engine/src/recovery/ with bounded ReAct loop"

## Context

The Validator (Article III gate) blocks the migration when the Mapper produced a plan that won't apply (unknown resource type, hallucinated attribute, missing required field). Stage 1 fails loud. Stage 2 introduces the Recovery agent — a bounded ReAct loop that asks the LLM to fix the errors, applies the proposed fixes, and re-validates.

Recovery is one of three named agents permitted by Article I (the others are Cost Optimizer and Cutover); adding any other agent requires a fresh RFC + stage-gate.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Single-error recovery succeeds in one iteration (Priority: P1)

The Validator reports one `UnknownAttribute` error. Recovery asks the LLM, which proposes a `set_attribute` call with the correct attribute name. After applying, re-validation passes. Recovery returns `Success { iterations: 1, fixes_applied: 1 }`.

**Why this priority**: Most common case in real migrations (single-attribute typo).

**Independent Test**: Stub LLM scripted to emit one `set_attribute` tool call; stub validator that flips from "fail" to "pass" after the fix is applied.

### User Story 2 — Multi-error recovery converges in N iterations (Priority: P1)

Validator reports three errors of mixed type. Recovery iterates: each iteration may fix some but not all. The agent keeps proposing fixes until all errors clear or `max_iterations` is hit.

**Why this priority**: Real migrations have many small errors that can't all be resolved in one ReAct cycle.

**Independent Test**: Stub LLM that fixes one error per iteration; verify Recovery converges in N iterations matching the error count.

### User Story 3 — Agent gives up gracefully (Priority: P2)

The LLM emits a final-answer turn (no tool calls) with errors still present — model self-reports it can't fix the problem. Recovery returns `AgentGaveUp` with the unresolved errors so the operator can see what's left.

**Why this priority**: Honest failure beats infinite loop. Same Article IV pattern as the kernel's `MaxTurnsReached`.

### User Story 4 — Max iterations exhausted (Priority: P2)

LLM keeps proposing fixes that don't actually resolve errors (e.g., loops between two wrong values). Recovery hits `max_iterations` and returns `MaxIterationsReached` with the remaining unresolved errors.

**Why this priority**: Hard cap that prevents runaway token cost (Article XII rule 4).

### Edge Cases

- LLM proposes a tool call with `target_addr` not in the plan → `FixApplier` returns `is_error: true`, kernel feeds error back to LLM, loop continues.
- `max_iterations = 0` → loud `RecoveryError::InvalidConfig` (Article IV).
- `WaitingForApproval` from kernel (Stage 2 narrow path: with `ToolApprovalPolicy::AcceptAll` this shouldn't happen, but if a custom policy is in play and a fix is gated, Recovery returns `WaitingForApproval` and the caller wires through the TUI).

## Requirements *(mandatory)*

- **FR-001**: `run_recovery` MUST re-run `Validator::validate` between every outer iteration — never trust kernel turn count alone.
- **FR-002**: Available fix tools MUST be: `set_attribute`, `change_target_type`, `remove_resource`. Unknown tool names MUST surface as `is_error: true` (not panic).
- **FR-003**: `max_iterations` MUST be a hard cap (Article XII rule 4); exceeding it returns `MaxIterationsReached` with unresolved errors.
- **FR-004**: When LLM emits final-answer turn while errors remain, Recovery MUST return `AgentGaveUp` (no silent success).
- **FR-005**: Plan mutations MUST be visible to subsequent iterations — `FixApplier` mutates an `Arc<Mutex<MappingPlan>>` shared between iterations.
- **FR-006**: `RecoveryError::InvalidConfig` MUST be returned for `max_iterations = 0` (loud config validation per Article IV).

## Success Criteria *(mandatory)*

- **SC-001**: 6+ unit tests for `FixApplier` (one per tool variant + error cases) all green.
- **SC-002**: 1+ test for prompt builder verifies all errors named in the prompt.
- **SC-003**: `cargo clippy -p terrashift-engine --all-targets -- -D warnings` passes.
- **SC-004**: `cargo test --workspace` remains green.
- **SC-005**: Token cost regression: 0 delta (kernel still inert from S9 baseline; outer-loop integration test deferred to S10b when AgentLlmClient adapter ships).

## Assumptions

- The `AgentLlmClient` adapter over `stakai::Inference` is **deferred to S10b** — this commit ships the structural Recovery shell with full unit coverage of `FixApplier`. The full outer-loop integration test (real LLM round-trip) lands when the adapter is in place.
- `ToolApprovalPolicy::AcceptAll` is the Stage 2 default for Recovery — operator review is via the audit log rather than per-call interactive approval. Custom policies can override.
- The Recovery prompt is Stage-2-narrowed (English, ad-hoc bullet list). Stage 5+ widens to a templated system prompt with cache-stable boundaries.

## Out of scope (deferred)

- Real LLM round-trip integration test → S10b
- Streaming Recovery output to the TUI → S13 (detached mode)
- Per-resource confidence scoring of fix proposals → Stage 5+
