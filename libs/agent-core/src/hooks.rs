//! Lifecycle hooks — observation seam for cross-cutting concerns (audit,
//! credential resolution, telemetry, cost tracking).
//!
//! Pattern: the architecture reference section 8 (kernel seam #3 from
//! TERRASHIFT_MAPPING.md §A — `AgentHook` with 5 default-no-op methods).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (verbatim).
//! Constitution: Article I (bounded blast radius via observation, not
//! mutation), V (audit hook is REQUIRED on `after_tool_execution`).
//!
//! Hook impls are registered with the agent loop at construction time and
//! fire in registration order. Each hook returns `Result<(), AgentError>` —
//! an `Err` aborts the entire run. Hooks **must not mutate the message list**
//! (that's the ContextReducer's job — Article XIII rule 1).
//!
//! Concrete hook impls land in:
//!   - `libs/audit/src/hooks.rs`  — AuditWriterHook (after_tool_execution)
//!   - `libs/creds/src/hooks.rs`  — SecretResolverHook (before_tool_execution)
//!   - Future: telemetry, cost tracking, etc.

use crate::{error::AgentError, types::AgentRunContext, types::ProposedToolCall};
use async_trait::async_trait;
use stakai::{Message, Model};

#[async_trait]
pub trait AgentHook: Send + Sync {
    async fn before_inference(
        &self,
        _run: &AgentRunContext,
        _messages: &[Message],
        _model: &Model,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn after_inference(
        &self,
        _run: &AgentRunContext,
        _messages: &[Message],
        _model: &Model,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn before_tool_execution(
        &self,
        _run: &AgentRunContext,
        _tool_call: &ProposedToolCall,
        _messages: &[Message],
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn after_tool_execution(
        &self,
        _run: &AgentRunContext,
        _tool_call: &ProposedToolCall,
        _messages: &[Message],
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn on_error(
        &self,
        _run: &AgentRunContext,
        _error: &AgentError,
        _messages: &[Message],
    ) -> Result<(), AgentError> {
        Ok(())
    }
}
