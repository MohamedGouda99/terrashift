//! `/migrate <plan_path>` — kick off a migration run.
//!
//! Pattern: refs/claude-code/src/commands/<corresponding>/index.ts.
//! Constitution: Article II, Article IV (empty path → friendly error).
//!
//! Stage 1: the dispatch contract is defined. The TUI runtime that
//! consumes `Action::StartMigration` arrives in Stage 2.

use super::{Action, CommandContext, CommandOutcome, SlashCommand};

pub struct Migrate;

impl SlashCommand for Migrate {
    fn name(&self) -> &'static str {
        "migrate"
    }

    fn description(&self) -> &'static str {
        "Start a migration run from <plan_path>"
    }

    fn run(&self, _ctx: &CommandContext, args: &str) -> CommandOutcome {
        let plan_path = args.trim();
        if plan_path.is_empty() {
            return CommandOutcome::Error(
                "/migrate requires a plan path: /migrate <plan_path.json>".to_string(),
            );
        }
        CommandOutcome::Action(Action::StartMigration {
            plan_path: plan_path.to_string(),
        })
    }
}
