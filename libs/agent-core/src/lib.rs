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
//! ## Module set
//!
//! Stage 1 ships every seam from `stakpak_arch.md §39 rows 1-5` that
//! has a Stage-1 consumer or a Default impl. The IMPLS that matter
//! arrive in later P-NN; the SEAMS ship now so consumers compile
//! against type-system-stable contracts.
//!
//! - `tools`      — `ToolExecutor` trait + `ToolExecutionResult` enum (P-02)
//! - `hooks`      — `AgentHook` trait, 5 lifecycle methods (P-02)
//! - `error`      — `AgentError` enum (P-02; grows per P-NN)
//! - `types`      — `AgentRunContext`, `ProposedToolCall`, `ToolDecision` (P-02)
//! - `registry`   — `ToolRegistry` (P-02; Terrashift addition; HashMap dispatch)
//! - `context`    — `ContextReducer` + `PassthroughContextReducer` (§A row 5; relocated from libs/engine/src/mapper/context.rs)
//! - `compaction` — `CompactionEngine` + `PassthroughCompactionEngine` (§A row 4; new this commit)
//!
//! ## Modules genuinely deferred (consumer-driven; ship when used)
//!
//! - `agent`      — `run_agent` loop (S9)
//! - `approval`   — `ApprovalStateMachine` (S9 — agent loop consumer)
//! - `checkpoint` — `CheckpointEnvelopeV1` (P-14 / S9)
//! - `retry`, `stream`, `budget_context` — (S9+)

pub mod compaction;
pub mod context;
pub mod error;
pub mod hooks;
pub mod registry;
pub mod tools;
pub mod types;

// Re-exports follow Stakpak's lib.rs pattern (subset).
pub use compaction::{CompactionEngine, CompactionResult, PassthroughCompactionEngine};
pub use context::{ContextReducer, Message, PassthroughContextReducer, Role};
pub use error::AgentError;
pub use hooks::AgentHook;
pub use registry::ToolRegistry;
pub use tools::{ToolExecutionResult, ToolExecutor};
pub use types::{AgentRunContext, ProposedToolCall, ToolDecision};
