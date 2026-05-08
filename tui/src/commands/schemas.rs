// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `/schemas` — emits a hint pointing operators at the canonical CLI surface.
//!
//! Per RFC schema-source-migration §OQ-2: `SlashCommand::run` is synchronous;
//! the `LocalSchemaStore` and `RuntimeManifest` operations the user wants are
//! either async or I/O-heavy. Rather than reshaping the trait, the TUI shows
//! current cache state in the status footer and points operators at the CLI
//! for any mutation.
//!
//! This matches the established pattern: `/migrate` and `/scan` similarly do
//! not run their heavy work inline.

use super::{CommandContext, CommandOutcome, SlashCommand};

pub struct Schemas;

impl SlashCommand for Schemas {
    fn name(&self) -> &'static str {
        "schemas"
    }

    fn description(&self) -> &'static str {
        "Show how to manage cached provider schemas (delegates to CLI)"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Hint(
            "Schema cache lives at ~/.terrashift/schemas/. Manage it from the shell:\n\
             \n  terrashift schema list                                   # what's cached\
             \n  terrashift schema update --provider aws --version 5.30.0 # fetch a version\
             \n  terrashift schema show aws@5.30.0 [--filter aws_iam]     # inspect a schema\
             \n  terrashift schema verify                                 # SHA-256 audit\
             \n  terrashift schema gc [--keep-versions 2]                 # prune old versions\
             \n\nThe TUI status footer shows the current cache count."
                .to_string(),
        )
    }
}
