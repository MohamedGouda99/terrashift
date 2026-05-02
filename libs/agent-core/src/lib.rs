//! Tool trait, agent loop primitives, lifecycle hooks.
//!
//! **The foundation that every Terrashift tool uses.**
//!
//! Pattern: stakpak_arch.md section 8 (THE CANONICAL AGENT LOOP — most
//! important section in the architecture document).
//! Source: refs/stakpak/libs/agent-core/src/lib.rs (subset — see
//! `specs/002-tool-trait-and-executor/spec.md` for the deferred modules).
//!
//! Constitution: Article I (architectural restraint — agents have bounded
//! blast radius via the seam model), Article II (mirror Stakpak), Article IV
//! (failures must be loud), Article XIII rule 3 (no unwrap/expect/string-slice
//! in production).
//!
//! ## P-02 module set (this commit)
//!
//! - `tools`    — `ToolExecutor` trait + `ToolExecutionResult` enum
//! - `hooks`    — `AgentHook` trait, 5 lifecycle methods (default no-op)
//! - `error`    — `AgentError` enum (minimal subset; grows per P-NN)
//! - `types`    — `AgentRunContext`, `ProposedToolCall`, `ToolDecision`
//! - `registry` — `ToolRegistry` (Terrashift addition; HashMap dispatch)
//!
//! ## Modules deferred to later P-NN (not yet present)
//!
//! - `agent`        — `run_agent` loop (Stage 2)
//! - `approval`     — `ApprovalStateMachine` (P-09)
//! - `compaction`   — `CompactionEngine` (Stage 2)
//! - `context`      — `ContextReducer` (Stage 2)
//! - `checkpoint`   — `CheckpointEnvelopeV1` (P-14)
//! - `retry`, `stream`, `budget_context` — (Stage 2)

pub mod error;
pub mod hooks;
pub mod registry;
pub mod tools;
pub mod types;

// Re-exports follow Stakpak's lib.rs pattern (subset).
pub use error::AgentError;
pub use hooks::AgentHook;
pub use registry::ToolRegistry;
pub use tools::{ToolExecutionResult, ToolExecutor};
pub use types::{AgentRunContext, ProposedToolCall, ToolDecision};
