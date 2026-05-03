//! `/checkpoint` — snapshot current run state for resume.
//!
//! Pattern: refs/claude-code/src/commands/<corresponding>/index.ts.
//! Cross-cutting: the architecture reference §22 (checkpoint envelope discipline);
//! the `Action::Checkpoint` variant maps to the V1 envelope shape that
//! ships with the agent loop kernel in S9.
//! Constitution: Article II.

use super::{Action, CommandContext, CommandOutcome, SlashCommand};

pub struct Checkpoint;

impl SlashCommand for Checkpoint {
    fn name(&self) -> &'static str {
        "checkpoint"
    }

    fn description(&self) -> &'static str {
        "Save current run state so it can be resumed later"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Action(Action::Checkpoint)
    }
}
