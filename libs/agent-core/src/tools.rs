//! Tool execution seam — the trait every executor (per-tool dispatcher,
//! ToolRegistry, sub-agent shells) implements.
//!
//! Pattern: stakpak_arch.md section 8 (THE CANONICAL AGENT LOOP — kernel-level
//! seam #2 from TERRASHIFT_MAPPING.md §A).
//! Source: refs/stakpak/libs/agent-core/src/tools.rs (verbatim — single
//! source of truth for the dispatch contract).
//! Constitution: Article I (architectural restraint), IV (failures loud).

use crate::{error::AgentError, types::AgentRunContext, types::ProposedToolCall};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

/// Outcome of a single tool invocation.
///
/// `is_error: true` on `Completed` means the tool ran to completion but
/// surfaced a domain error (e.g., HCL parse failed, schema mismatch). The
/// LLM still gets the result and can react. `Cancelled` is reserved for
/// `CancellationToken`-driven aborts and is propagated up to the runtime
/// without LLM observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionResult {
    Completed { result: String, is_error: bool },
    Cancelled,
}

/// The single seam through which tool calls reach concrete code.
///
/// Implementations dispatch on `tool_call.name`. The runtime guarantees:
///   - `tool_call.id` is unique within the run
///   - `tool_call.arguments` is JSON the implementation must validate
///   - `cancel` may fire mid-execution; honor it promptly
///
/// Returning `Err(AgentError::*)` aborts the entire run. For per-tool errors
/// that the LLM should see, return `Ok(Completed { is_error: true, ... })`.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute_tool_call(
        &self,
        run: &AgentRunContext,
        tool_call: &ProposedToolCall,
        cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError>;
}
