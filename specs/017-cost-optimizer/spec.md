# Feature Specification: Cost Optimizer Agent (S11)

**Feature Branch**: `017-cost-optimizer`
**Created**: 2026-05-03
**Status**: Draft
**Input**: SESSION_PLAN.md S11 — "Cost Optimizer + Infracost service"

## Context

Cross-cloud migrations often shift cost dramatically — sometimes up. The Cost Optimizer agent reads an Infracost-style estimate of the migration plan and asks the LLM to propose right-sizing/storage-class/architecture changes that bring the cost under an operator-set target without breaking correctness.

Cost Optimizer is one of three named agents permitted by Article I (Recovery, Cost Optimizer, Cutover). Same two-budget structure as Recovery (max_iterations outer + max_turns inner).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Plan already under target (Priority: P1)

Plan estimate is $80/month, target is $100/month. Optimizer skips the loop entirely and returns `Success { iterations: 0 }`. No tool calls fire.

### User Story 2 — Single-iteration optimization (Priority: P1)

Plan is $300/month, target is $200. LLM proposes one `set_attribute` call (e.g., `t3.large` → `t3.medium`). After applying, re-cost shows $180. Optimizer returns `Success { iterations: 1, fixes_applied: 1 }`.

### User Story 3 — Hard cap on iterations (Priority: P2)

LLM keeps proposing changes that don't reduce cost enough. After `max_iterations`, optimizer returns `MaxIterationsReached` with the partial-state plan and current cost so the operator can review.

### User Story 4 — Agent gives up (Priority: P2)

LLM emits final-answer turn with no tool calls — model couldn't find a safe optimization. Optimizer returns `AgentGaveUp` with current cost.

### Edge Cases

- `target_monthly_usd < 0` → loud `OptimizerError::InvalidConfig`.
- `max_iterations = 0` → loud `OptimizerError::InvalidConfig`.
- `WaitingForApproval` from kernel (custom approval policies) → returned as `OptimizationOutcome::WaitingForApproval`.

## Requirements

- **FR-001**: `run_optimizer` MUST cost the plan once before the loop; if already under target, exit `Success { iterations: 0 }`.
- **FR-002**: Each iteration MUST re-cost via the `CostService` trait — no trusting cached values across iterations.
- **FR-003**: Available tools MUST be `set_attribute` and `remove_resource`. No invented tools.
- **FR-004**: `OptimizerError::InvalidConfig` MUST fire for `max_iterations = 0` or `target_monthly_usd < 0`.
- **FR-005**: A `StubCostService` MUST exist for hermetic testing — `InfracostCostService` lands in S11b once the API key is procured.

## Success Criteria

- **SC-001**: 4+ unit tests covering StubCostService, prompt builder, FixApplier mutation paths.
- **SC-002**: `cargo clippy -p terrashift-engine --all-targets -- -D warnings` passes.
- **SC-003**: `cargo test --workspace` remains green.

## Assumptions

- The `InfracostCostService` (real https://api.infracost.io adapter) is **deferred to S11b** pending an Infracost API key.
- The Cost Optimizer prompt is Stage-2-narrowed (English + bullet patterns); Stage 5+ adds explicit per-cloud right-sizing knowledge.
- `ToolApprovalPolicy::AcceptAll` is the Stage-2 default; production deployments override via profile.

## Out of scope

- Real Infracost integration → S11b
- Per-region cost breakdown / committed-use savings → Stage 5+
- TUI surface for cost diff visualization → S13 (detached mode TUI)
