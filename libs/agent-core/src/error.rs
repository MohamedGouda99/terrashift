// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Top-level agent error.
//!
//! Pattern: the architecture reference section 8 (kernel error taxonomy).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (subset — see clarify Q4).
//! Constitution: Article IV (failures must be loud — every public method
//! returning Result<_, AgentError> is auditable).
//!
//! P-02 scope: only the variants needed by `tools.rs`, `hooks.rs`,
//! `registry.rs`. the reference's full enum has variants for approval, checkpoint,
//! stream — those modules don't exist yet, so their `From` impls would not
//! compile. Each variant arrives with its module:
//!   - Approval     → S9 (this commit; agent loop kernel)
//!   - Checkpoint   → P-14 (TUI slash commands /checkpoint)
//!   - Stream       → Stage 5+ (deferred per S9 spec FR-010)
//!   - Compaction   → S9 (this commit; threshold-triggered)

use crate::approval::ApprovalError;
use thiserror::Error;

/// Errors that propagate out of the agent kernel.
///
/// Variant signatures match `the reference codebase (see ATTRIBUTIONS.md)`
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

    // S9 additions — agent loop kernel error variants.
    /// Approval state machine rejected a command (unknown id, double
    /// resolution, etc.). Surfaces from `ApprovalStateMachine::apply_command`.
    #[error("approval error: {0}")]
    Approval(#[from] ApprovalError),

    /// LLM emitted two `ProposedToolCall` entries with the same `id` in
    /// the same turn. Article XIII rule 9 enforcement boundary.
    #[error("duplicate tool_call_id within a single turn: {tool_call_id}")]
    DuplicateToolCallId { tool_call_id: String },

    /// Caller passed a config that the kernel can't honor (e.g.,
    /// `max_turns = 0`). Loud validation error per Article IV.
    #[error("invalid agent loop config: {0}")]
    InvalidConfig(String),

    /// LLM call retried `max_attempts` times and never succeeded.
    /// Surfaces the last underlying error message + the attempt count.
    #[error("LLM retry budget exhausted after {attempts} attempt(s); last error: {last_error}")]
    LlmRetryExhausted { attempts: usize, last_error: String },

    /// Compaction engine returned an error. Stage 2 uses Passthrough so
    /// this only fires when a Stage 5+ custom compactor goes wrong.
    #[error("compaction failed: {0}")]
    Compaction(String),
}
