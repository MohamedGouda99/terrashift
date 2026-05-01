---
name: rust-tool-impl
description: Loaded whenever you're implementing a new Rust Tool for Terrashift. Use when adding a new tool to libs/agent-core or libs/engine.
---

When implementing a Rust Tool for Terrashift:

1. **Read the trait** — `libs/agent-core/src/tool.rs` to understand the trait surface (filled by P-02).
2. **Read an analogous Stakpak tool** — `~/refs/stakpak/libs/agent-core/` for a similar tool. Use `reference-explorer` sub-agent if needed.
3. **Define typed I/O** — input/output structs with `serde + schemars` so JSON Schema is auto-generated. Mismatch between schema and type is a compile error (per terrashift_plan.md §16.5).
4. **Implement the trait `async`** — use `async-trait`. Return `Result<ToolExecutionResult, ToolError>`.
5. **Tests cover four cases:**
   - Happy path
   - Malformed input (schema validation rejects)
   - Error from downstream call
   - Cancellation (the `CancellationToken` is honored)
6. **Register the tool** — add to `ToolRegistry` in `libs/agent-core/src/registry.rs`.
7. **PR description** — cite Article I (architectural restraint — confirm this isn't an agent if it shouldn't be) + `stakpak_arch.md` section 8.

Constitutional checklist for every tool:

- Article IV: failures via `Result`, no `unwrap()`/`expect()` (Article XIII rule 3).
- Article V: if the tool touches credentials, use `{{secret:name}}` references; resolve via `libs/creds`. Don't hold raw values.
- Article X: emit a `tracing` span for the tool execution.
- Article XII: if the tool calls an LLM, route via tier (`Tier::Eco` or `Tier::Smart`).
