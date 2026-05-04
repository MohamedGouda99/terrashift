// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `/help` — list every registered slash command + description.
//!
//! Pattern: refs/claude-code/src/commands/help/index.ts (per-file
//! metadata + handler). Per TERRASHIFT_MAPPING.md §F1.
//! Constitution: Article II (Claude Code borrow), Article IV.

use super::{CommandContext, CommandOutcome, SlashCommand};

pub struct Help;

impl SlashCommand for Help {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Show help and available slash commands"
    }

    fn run(&self, ctx: &CommandContext, _args: &str) -> CommandOutcome {
        let mut out = String::from("Available commands:\n");
        for (name, desc) in ctx.registry.list() {
            out.push_str(&format!("  /{:<11} {}\n", name, desc));
        }
        CommandOutcome::Text(out)
    }
}
