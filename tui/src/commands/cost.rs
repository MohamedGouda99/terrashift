//! `/cost` — pre/post migration cost diff (Stage 2+ stub).
//!
//! Pattern: refs/claude-code/src/commands/cost/index.ts.
//! Cross-cutting: Infracost integration (terrashift_plan.md §9 +
//! P-11 Cost Optimizer), shipping in S11.

use super::{CommandContext, CommandOutcome, SlashCommand};

pub struct Cost;

impl SlashCommand for Cost {
    fn name(&self) -> &'static str {
        "cost"
    }

    fn description(&self) -> &'static str {
        "Show cost analysis for the current migration (Stage 2+)"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Text(
            "Stage 2+: /cost will surface Infracost-driven before/after spend\n\
             analysis once the Cost Optimizer agent ships in S11. Currently\n\
             not implemented."
                .to_string(),
        )
    }
}
