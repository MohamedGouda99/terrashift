# Tasks — P-02

Atomic dependency-ordered tasks. Sequence matches plan.md "Build order".

## Tasks

- [x] T1: Verify `refs/stakpak/libs/agent-core/src/{tools,hooks,error,types}.rs` (already read in plan)
- [ ] T2: Modify `libs/agent-core/Cargo.toml` — add `stakai = { workspace = true }`
- [ ] T3: Write `libs/agent-core/src/types.rs` — `AgentRunContext`, `ProposedToolCall`, `ToolDecision` (Stakpak verbatim subset)
- [ ] T4: Write `libs/agent-core/src/error.rs` — minimal `AgentError` (4 variants)
- [ ] T5: Write `libs/agent-core/src/tools.rs` — `ToolExecutor` trait + `ToolExecutionResult` enum (Stakpak verbatim)
- [ ] T6: Write `libs/agent-core/src/hooks.rs` — `AgentHook` trait, 5 lifecycle methods (Stakpak verbatim)
- [ ] T7: Write `libs/agent-core/src/registry.rs` — `ToolRegistry` (Terrashift addition, cited)
- [ ] T8: Rewrite `libs/agent-core/src/lib.rs` — module declarations + re-exports (Stakpak pattern)
- [ ] T9: Write `libs/agent-core/tests/echo_executor_test.rs` — 4 test cases (happy, cancellation, registry dispatch, registry not found)
- [ ] T10: `cargo check -p terrashift-agent-core` (must pass)
- [ ] T11: `cargo test -p terrashift-agent-core` (4 cases pass)
- [ ] T12: `cargo clippy --all-targets -- -D warnings` (must pass)
- [ ] T13: `cargo fmt -- --check` (must pass)
- [ ] T14: Write analyze.md + checklist.md
- [ ] T15: Commit `feat(p02): ToolExecutor + AgentHook + ToolRegistry`

## Dependencies

- T2 → T3,T4,T5,T6,T7,T8 (Cargo.toml must have stakai for hooks.rs to compile)
- T3,T4 → T5 (tools.rs uses both)
- T3,T4 → T6 (hooks.rs uses error + types + stakai)
- T5 → T7 (registry impls ToolExecutor)
- T3-T7 → T8 (lib.rs re-exports from all)
- T8 → T9 (test imports via lib re-exports)
- T9 → T10-T13 (verification)
- T10-T13 → T14-T15 (commit only after all gates pass)
