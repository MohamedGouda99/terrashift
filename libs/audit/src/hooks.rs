//! `AuditWriterHook` — wires `AgentHook` lifecycle to audit log emission.
//!
//! Pattern: terrashift_plan.md §6.X integration ("after_tool_execution hook").
//! Constitution: Article V (audit on every tool execution).
//!
//! Registered with the agent loop in S9 (`run_agent` ships):
//!
//! ```ignore
//! let audit_hook = AuditWriterHook::new(audit_store, signer, run_id);
//! agent_config.hooks.push(Box::new(audit_hook));
//! ```

use crate::entry::{Actor, AuditEntry, AuditPayload, Outcome};
use crate::signer::SessionSigner;
use crate::store::AuditStore;
use async_trait::async_trait;
use std::sync::Arc;
use terrashift_agent_core::{AgentError, AgentHook, AgentRunContext, ProposedToolCall};
use uuid::Uuid;

/// Bridges agent-core's `AgentHook` lifecycle to audit log writes.
///
/// Currently emits `ToolExecution` entries on `after_tool_execution`.
/// `LlmCall` emission lives in `libs/ai`'s wrapper; this hook can't see
/// LLM details (it observes tool calls, not inferences).
pub struct AuditWriterHook {
    store: Arc<dyn AuditStore>,
    signer: Arc<SessionSigner>,
    run_id: Uuid,
}

impl AuditWriterHook {
    pub fn new(store: Arc<dyn AuditStore>, signer: Arc<SessionSigner>, run_id: Uuid) -> Self {
        Self {
            store,
            signer,
            run_id,
        }
    }
}

#[async_trait]
impl AgentHook for AuditWriterHook {
    async fn after_tool_execution(
        &self,
        _run: &AgentRunContext,
        tool_call: &ProposedToolCall,
        _messages: &[stakai::Message],
    ) -> Result<(), AgentError> {
        // Stage 1: emit a minimal ToolExecution entry. Duration tracking is
        // approximate (we don't know exact start time here; future work will
        // track it via `before_tool_execution`). cred_ref is None since this
        // hook can't see credential metadata; libs/creds emits its own
        // `CredentialResolution` entries on resolve.
        let payload = AuditPayload::ToolExecution {
            tool_name: tool_call.name.clone(),
            cred_ref: None,
            target: tool_call.id.clone(),
            duration_ms: 0,
        };

        let mut entry = AuditEntry::new(
            self.run_id,
            Actor::Agent {
                name: "agent-core".to_string(),
            },
            "tool.execute",
            Outcome::Ok,
            payload,
        );

        self.store
            .append(&self.signer, &mut entry)
            .await
            .map_err(|e| AgentError::Hook(e.to_string()))?;

        Ok(())
    }
}
