//! Top-level agent error.
//!
//! Pattern: stakpak_arch.md section 8 (kernel error taxonomy).
//! Source: refs/stakpak/libs/agent-core/src/error.rs (subset — see clarify Q4).
//! Constitution: Article IV (failures must be loud — every public method
//! returning Result<_, AgentError> is auditable).
//!
//! P-02 scope: only the variants needed by `tools.rs`, `hooks.rs`,
//! `registry.rs`. Stakpak's full enum has variants for approval, checkpoint,
//! stream — those modules don't exist yet, so their `From` impls would not
//! compile. Each variant arrives with its module:
//!   - Approval     → P-09 (Executor)
//!   - Checkpoint   → P-14 (TUI slash commands /checkpoint)
//!   - Stream       → Stage 2 (run_agent loop)
//!   - Compaction   → Stage 2 (CompactionEngine)

use thiserror::Error;

/// Errors that propagate out of the agent kernel.
///
/// Variant signatures match `refs/stakpak/libs/agent-core/src/error.rs`
/// exactly — future expansion is purely additive (new variants, never
/// renamed).
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("inference failed: {0}")]
    Inference(String),

    #[error("hook failed: {0}")]
    Hook(String),

    #[error("tool execution failed: {0}")]
    ToolExecution(String),

    #[error("invalid command: {0}")]
    InvalidCommand(String),

    #[error("run cancelled")]
    Cancelled,
}
