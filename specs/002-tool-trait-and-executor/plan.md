# Implementation Plan — P-02

## File-by-file plan (mirror order)

| # | File | Source pattern | Lines (est) | Notes |
|---|---|---|---|---|
| 1 | `libs/agent-core/Cargo.toml` (modify) | — | +1 | Add `stakai = { workspace = true }` |
| 2 | `libs/agent-core/src/types.rs` | `refs/stakpak/libs/agent-core/src/types.rs` lines 1-12, 311-343 (subset) | ~40 | AgentRunContext, ProposedToolCall, ToolDecision — verbatim |
| 3 | `libs/agent-core/src/error.rs` | `refs/stakpak/libs/agent-core/src/error.rs` (subset) | ~20 | Minimal: Inference, Hook, ToolExecution, Cancelled (per clarify Q4) |
| 4 | `libs/agent-core/src/tools.rs` | `refs/stakpak/libs/agent-core/src/tools.rs` (verbatim) | ~20 | ToolExecutor trait + ToolExecutionResult enum |
| 5 | `libs/agent-core/src/hooks.rs` | `refs/stakpak/libs/agent-core/src/hooks.rs` (verbatim) | ~50 | AgentHook trait, 5 lifecycle methods |
| 6 | `libs/agent-core/src/registry.rs` | **Terrashift addition** | ~80 | ToolRegistry struct + impl ToolExecutor |
| 7 | `libs/agent-core/src/lib.rs` (rewrite) | `refs/stakpak/libs/agent-core/src/lib.rs` (subset) | ~30 | mod declarations + re-exports |
| 8 | `libs/agent-core/tests/echo_executor_test.rs` | new | ~80 | EchoExecutor + 4 test cases |

Total: ~320 lines including tests.

## Build order (avoids broken intermediate states)

1. Modify `Cargo.toml` (add stakai)
2. Write `types.rs` (no deps on other agent-core modules — leaf)
3. Write `error.rs` (no deps)
4. Write `tools.rs` (deps on error, types — both written)
5. Write `hooks.rs` (deps on error, types, stakai — all available)
6. Write `registry.rs` (deps on tools — written)
7. Rewrite `lib.rs` to declare modules + re-export
8. Write `tests/echo_executor_test.rs` (uses public re-exports from lib)
9. `cargo check -p terrashift-agent-core` (must pass)
10. `cargo test -p terrashift-agent-core` (4 cases must pass)
11. `cargo clippy --all-targets -- -D warnings` (workspace-wide, must pass)
12. `cargo fmt -- --check` (workspace-wide, must pass)
13. Commit

## Constitution articles enforced

- **Article I** (architectural restraint) — ToolExecutor is the primitive;
  agents are just impls. No new agent introduced in P-02.
- **Article IV** (failures loud) — All public methods return `Result<_, AgentError>`.
- **Article XIII rule 3** — Verified by clippy (deny lints already in workspace Cargo.toml).
- **Article II** — Source files cite `// Pattern: stakpak_arch.md section 8`
  + `// Source: refs/stakpak/libs/agent-core/src/<file>.rs`.

## Citation discipline

Every source file's doc comment includes:
```rust
//! Pattern: stakpak_arch.md section 8 (the canonical agent loop kernel).
//! Source: refs/stakpak/libs/agent-core/src/<file>.rs
//! Constitution: Article I, IV, XIII rule 3.
```

The `registry.rs` (Terrashift addition) cites:
```rust
//! Terrashift addition — not in Stakpak. Convenience layer over ToolExecutor
//! for multi-tool registration. See specs/002-tool-trait-and-executor/clarify.md Q3.
```
