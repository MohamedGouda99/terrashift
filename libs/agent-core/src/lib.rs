//! Tool trait, agent loop primitives, conversation state.
//!
//! This is **the foundation that every Terrashift tool uses**.
//!
//! Pattern: stakpak_arch.md section 8 (THE CANONICAL AGENT LOOP — most
//! important section in the architecture document).
//!
//! Constitution: Article I (architectural restraint — agents have bounded
//! blast radius via the subagent permission model), Article IV (failures
//! must be loud), Article XIII rule 3 (no unwrap/expect/string-slice in
//! production).
//!
//! Modules to be filled in by P-NN prompts:
//! - `tool.rs` — Tool trait (P-02)
//! - `executor.rs` — ToolExecutor trait per section 8 (P-02)
//! - `registry.rs` — ToolRegistry; name → Tool dispatch (P-02)
//! - `errors.rs` — ToolError per section 8 error taxonomy (P-02)
//! - `agent_loop.rs` — generic LLM-with-tool-use loop (Recovery + Cost Optimizer agents, Stage 2)
//! - `conversation.rs` — turn-based state machine (per Claude Code QueryEngine)
//! - `permission.rs` — ApprovalStateMachine (per stakpak_arch.md section 8)
//! - `checkpoint.rs` — session serialization (P-14, per section 22)
//! - `compact.rs` — three-mode compaction (Claude Code services/compact pattern)
//! - `hooks.rs` — 5 lifecycle hooks: before_inference, after_inference,
//!   before_tool_execution, after_tool_execution, on_error

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
