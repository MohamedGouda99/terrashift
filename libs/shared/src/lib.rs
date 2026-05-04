// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cross-cutting types and utilities used by every Terrashift crate.
//!
//! This is a **leaf crate** — no runtime dependencies on other workspace
//! members. That keeps the workspace dependency graph acyclic and lets
//! every other crate depend on `shared` freely.
//!
//! Pattern: the architecture reference section 9 (shared crate role).
//! Constitution: Article XIII rule 4 (don't conflate ChatMessage and
//! LLMMessage — storage type vs runtime type, enforced at this boundary).
//!
//! Modules to be filled in by P-NN prompts:
//! - `types.rs` — ResourceType, ProviderName, Region (P-04)
//! - `errors.rs` — shared error taxonomy (P-02)
//! - `ids.rs` — MigrationId, CheckpointId, AuditEntryId (P-11)
//! - `files.rs` — backup-first file ops (P-08, per the architecture reference section 28)
//! - `redaction.rs` — privacy-mode regex set (P-10, per section 27)
//! - `tracing_init.rs` — tracing-subscriber init (used by every binary)

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
