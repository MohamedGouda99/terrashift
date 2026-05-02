//! `/rollback <run_id>` — restore a prior run's pre-migration state.
//!
//! Pattern: refs/claude-code/src/commands/<corresponding>/index.ts.
//! Cross-cutting: leverages `Generator::rollback` (P-08) and audit-log
//! `verify_chain` to confirm the run_id is internally consistent before
//! we touch any backed-up files. Real wiring in Stage 5 when the CLI
//! threads `Arc<dyn AuditStore>` + `Generator` into `CommandContext`.
//!
//! Constitution: Article IX (backups archival; rollback is the explicit
//! restore path), Article IV (loud error on missing arg).

use super::{Action, CommandContext, CommandOutcome, SlashCommand};

pub struct Rollback;

impl SlashCommand for Rollback {
    fn name(&self) -> &'static str {
        "rollback"
    }

    fn description(&self) -> &'static str {
        "Restore a prior run's pre-migration state (Stage 2+)"
    }

    fn run(&self, _ctx: &CommandContext, args: &str) -> CommandOutcome {
        let run_id = args.trim();
        if run_id.is_empty() {
            return CommandOutcome::Error(
                "/rollback requires a run_id: /rollback <uuid>".to_string(),
            );
        }
        CommandOutcome::Action(Action::Rollback {
            run_id: run_id.to_string(),
        })
    }
}
