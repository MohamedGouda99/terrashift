// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `/audit` — audit log inspection (Stage 1: placeholder text).
//!
//! Pattern: refs/claude-code/src/commands/<corresponding>/index.ts.
//! Constitution: Article II (Claude Code borrow), Article V (audit
//! is the canonical compliance surface — citing audit log location
//! here keeps operators discoverable).
//!
//! Stage 5+ wires real `LocalAuditStore::verify_chain` /
//! `export(run_id)` / `query(filter)` once the CLI carries an
//! `Arc<dyn AuditStore>` in `CommandContext`.

use super::{CommandContext, CommandOutcome, SlashCommand};

pub struct Audit;

impl SlashCommand for Audit {
    fn name(&self) -> &'static str {
        "audit"
    }

    fn description(&self) -> &'static str {
        "Inspect the signed audit log (chain integrity, export, query)"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Text(
            "Audit log lives at .terrashift/audit.db (per-run signed Ed25519 chain).\n\
             Stage 1 placeholder: subcommands /audit verify, /audit export, /audit query\n\
             arrive in Stage 5 when the CLI wires LocalAuditStore into the command\n\
             context. For now: query the SQLite file directly."
                .to_string(),
        )
    }
}
