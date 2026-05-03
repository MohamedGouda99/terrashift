//! `/plan` — preview a migration before apply (Stage 2+ stub).
//!
//! Pattern: refs/claude-code/src/commands/<corresponding>/index.ts.
//! Cross-cutting: maps to the reference's plan-mode lifecycle
//! (the architecture reference §16); arrives in Stage 2 when the TUI runtime
//! ships the plan-mode UI.

use super::{CommandContext, CommandOutcome, SlashCommand};

pub struct PlanCmd;

impl SlashCommand for PlanCmd {
    fn name(&self) -> &'static str {
        "plan"
    }

    fn description(&self) -> &'static str {
        "Preview a migration's HCL output before apply (Stage 2+)"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Text(
            "Stage 2+: /plan will surface the Mapper + Generator output for review\n\
             before /migrate apply. Currently not implemented."
                .to_string(),
        )
    }
}
