//! `/quit` (alias `/exit`) — request graceful TUI shutdown.

use super::{Action, CommandContext, CommandOutcome, SlashCommand};

pub struct Quit;

impl SlashCommand for Quit {
    fn name(&self) -> &'static str {
        "quit"
    }

    fn description(&self) -> &'static str {
        "Exit Terrashift (alias: /exit, Ctrl+C)"
    }

    fn run(&self, _ctx: &CommandContext, _args: &str) -> CommandOutcome {
        CommandOutcome::Action(Action::Exit)
    }
}
