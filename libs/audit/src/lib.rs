//! Terrashift signed, hash-chained, append-only audit log.
//!
//! Every meaningful operation in Terrashift writes an `AuditEntry` here.
//! The writer is the LAST line of defence against secret leakage (Article
//! XIII rule 5 — pre-write scrubber panics on detected raw secrets).
//!
//! Pattern: terrashift_plan.md §6.X (full schema + invariants).
//! Source: refs/stakpak/libs/agent-core/src/checkpoint.rs (envelope discipline).
//! Constitution: Article V (audit invariant), IX (data governance —
//! append-only, never auto-deleted), XIII rules 3 + 5.
//!
//! ## Module map
//!
//! - `entry`     — AuditEntry, AuditPayload, Actor, Outcome, FileOpKind
//! - `errors`    — AuditError enum
//! - `signer`    — Ed25519 SessionSigner (per-migration keypair) + verify
//! - `scrubber`  — pre-write secret detection (panics on hit)
//! - `store`     — AuditStore trait + LocalAuditStore (sqlx + sqlite)
//! - `verify`    — `verify_chain` walks exported entries
//! - `hooks`     — AuditWriterHook implements terrashift_agent_core::AgentHook

pub mod entry;
pub mod errors;
pub mod hooks;
pub mod scrubber;
pub mod signer;
pub mod store;
pub mod verify;

pub use entry::{Actor, AuditEntry, AuditPayload, FileOpKind, Outcome};
pub use errors::AuditError;
pub use hooks::AuditWriterHook;
pub use scrubber::{ScrubMatch, PATTERN_NAMES};
pub use signer::{verify_signature, SessionSigner};
pub use store::{compute_content_hash, AuditStore, LocalAuditStore};
pub use verify::verify_chain;
