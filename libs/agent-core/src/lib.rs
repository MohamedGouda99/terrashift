//! Tool trait, agent loop primitives, lifecycle hooks.
//!
//! **The foundation that every Terrashift tool uses.**
//!
//! Pattern: the architecture reference section 8 (THE CANONICAL AGENT LOOP — most
//! important section in the architecture document).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (subset — see
//! `specs/002-tool-trait-and-executor/spec.md` for the deferred modules).
//!
//! Constitution: Article I (architectural restraint — agents have bounded
//! blast radius via the seam model), Article II (mirror the reference), Article IV
//! (failures must be loud), Article XIII rule 3 (no unwrap/expect/string-slice
//! in production).
//!
//! ## Module set
//!
//! Stage 1 ships every seam from `the architecture reference §39 rows 1-5` that
//! has a Stage-1 consumer or a Default impl. S9 (this commit) adds
//! the agent-loop kernel modules so Stage 2 consumers (S10 Recovery
//! agent, S11 Cost Optimizer) can compile against a type-system-stable
//! contract.
//!
//! - `tools`      — `ToolExecutor` trait + `ToolExecutionResult` enum (P-02)
//! - `hooks`      — `AgentHook` trait, 5 lifecycle methods (P-02)
//! - `error`      — `AgentError` enum (P-02; S9 extends with Approval, DuplicateToolCallId, InvalidConfig, LlmRetryExhausted, Compaction)
//! - `types`      — Run context, tool call, decision, approval policy, retry config, agent commands, loop config + result (P-02 + S9)
//! - `registry`   — `ToolRegistry` (P-02; Terrashift addition; HashMap dispatch)
//! - `context`    — `ContextReducer` + `PassthroughContextReducer` (§A row 5)
//! - `compaction` — `CompactionEngine` + `PassthroughCompactionEngine` (§A row 4)
//! - `approval`   — `ApprovalStateMachine` for ordered tool dispatch (S9)
//! - `retry`      — exponential backoff + retry-after parsing (S9)
//! - `agent`      — `run_agent` kernel + `AgentLlmClient` trait (S9)
//!
//! ## Modules deliberately deferred to S5+
//!
//! - `stream`         — Stage 2 consumers don't stream; widens the LLM trait when they do
//! - `checkpoint`     — `CheckpointEnvelopeV1` (P-14 / TUI integration)
//! - `budget_context` — budget-aware reducer (replaces Passthrough in Stage 5)

pub mod agent;
pub mod approval;
pub mod compaction;
pub mod context;
pub mod error;
pub mod hooks;
pub mod registry;
pub mod retry;
pub mod tools;
pub mod types;

// Re-exports follow the reference's lib.rs pattern (subset).
pub use agent::{
    run_agent, AgentLlmClient, AgentMessage, AgentToolDef, LlmTurnError, LlmTurnOutcome,
};
pub use approval::{ApprovalError, ApprovalStateMachine, ResolvedToolCall};
pub use compaction::{CompactionEngine, CompactionResult, PassthroughCompactionEngine};
pub use context::{ContextReducer, Message, PassthroughContextReducer, Role};
pub use error::AgentError;
pub use hooks::AgentHook;
pub use registry::ToolRegistry;
pub use retry::{
    exponential_backoff_ms, parse_retry_delay_from_headers, resolve_retry_delay_ms, RetryDelay,
    RetryDelaySource,
};
pub use tools::{ToolExecutionResult, ToolExecutor};
pub use types::{
    AgentCommand, AgentLoopConfig, AgentLoopReason, AgentLoopResult, AgentRunContext,
    ProposedToolCall, RetryConfig, ToolApprovalAction, ToolApprovalPolicy, ToolDecision,
};
