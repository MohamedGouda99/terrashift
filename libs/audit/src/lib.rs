//! Signed, hash-chained, append-only audit log.
//!
//! Pattern: stakpak_arch.md section 22 (checkpoint and resume — same
//! lifecycle hook points; the audit chain is INDEPENDENT of the checkpoint
//! chain — losing a checkpoint must not break audit verification).
//!
//! Constitution: Article V (audit), Article IX (data governance — audit logs
//! are user property, never auto-deleted). Article XIII rules 3, 5.
//!
//! Schema: AuditEntry { id, timestamp, run_id, actor, operation, outcome,
//! payload: AuditPayload, prev_hash, content_hash, signature: [u8; 64] }
//!
//! AuditPayload variants: LlmCall, ToolExecution, PhaseTransition,
//! CredentialResolution, FileOperation. See terrashift_plan.md §6.X for the
//! full payload schema.
//!
//! Redaction invariant (per Article XIII rule 5): the audit writer is the
//! LAST line of defence — it runs the pre-prompt scrubber over every string
//! field and panics loudly if any field contains a gitleaks-detected pattern.
//!
//! Modules to be filled in by P-NN prompts (P-11):
//! - `entry.rs` — AuditEntry struct, AuditPayload enum
//! - `chain.rs` — SHA-256 prev_hash + content_hash
//! - `signer.rs` — Ed25519 per-migration key
//! - `store.rs` — SQLite append-only store
//! - `verify.rs` — chain verification on demand
//! - `export.rs` — compliance review export
//! - `query.rs` — filter by run_id, time range, payload variant

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
