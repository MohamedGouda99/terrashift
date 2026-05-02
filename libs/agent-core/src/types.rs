//! Core agent types: run context, proposed tool calls, decisions.
//!
//! Pattern: stakpak_arch.md section 8 (the canonical agent loop kernel).
//! Source: refs/stakpak/libs/agent-core/src/types.rs (subset — only what P-02 needs).
//! Constitution: Article I (architectural restraint), II (verbatim mirror).
//!
//! Stage 1 P-02 scope: only the types referenced by `tools.rs`, `hooks.rs`,
//! `registry.rs`. Other Stakpak types (AgentConfig, AgentEvent, AgentCommand,
//! ToolApprovalPolicy, etc.) arrive in later P-NN prompts when their consuming
//! module ships.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-run identity passed into every `ToolExecutor::execute_tool_call` and
/// every `AgentHook` lifecycle method.
///
/// Verbatim from Stakpak (`refs/stakpak/libs/agent-core/src/types.rs:8-12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunContext {
    pub run_id: Uuid,
    pub session_id: Uuid,
}

/// A tool call proposed by the LLM, before approval and execution.
///
/// Verbatim from Stakpak (`refs/stakpak/libs/agent-core/src/types.rs:336-343`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// User decision on a proposed tool call, returned by the approval flow.
///
/// Verbatim from Stakpak (`refs/stakpak/libs/agent-core/src/types.rs:311-315`).
/// Used in P-09 (Executor) when approval state machine arrives. Defined here
/// in P-02 so the public type surface is stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolDecision {
    Accept,
    Reject,
    CustomResult { content: String },
}
