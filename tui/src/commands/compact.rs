// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `/compact` — compress conversation context (Stage 2+ stub).
//!
//! Pattern: refs/claude-code/src/commands/compact/. Per
//! TERRASHIFT_MAPPING.md §F2, Claude Code splits compaction into
//! manual / threshold / reactive entry points — Stage 2+ Terrashift
//! adopts the same split. Stage 1 wires only the manual entry.
//! Cross-cutting: the architecture reference §8 (CompactionEngine).

use super::{Action, CommandContext, CommandOutcome, SlashCommand};

pub struct Compact;

impl SlashCommand for Compact {
    fn name(&self) -> &'static str {
        "compact"
    }

    fn description(&self) -> &'static str {
        "Compact conversation context to reclaim window space (Stage 2+)"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Action(Action::Compact)
    }
}
