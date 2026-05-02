//! Slash-command dispatch — per-file pattern adopted from Claude Code.
//!
//! Pattern: refs/claude-code/src/commands/<name>/{index.ts, <name>.ts}
//!         (metadata + handler split, lazy-loaded). Adapted to Rust as
//!         one `.rs` per command — no lazy load needed (Rust compiles
//!         to one binary). Per TERRASHIFT_MAPPING.md §F1, this is the
//!         *one* place we adopt Claude Code over Stakpak: Stakpak's
//!         centralised match in `cli/src/commands/mod.rs` requires
//!         3 edit-sites per new command; ours stays at 1 file + 1
//!         registry line.
//! Stakpak counterpart: stakpak_arch.md §16 (TUI structure overall).
//!
//! Constitution: Article II (Claude Code conceptual borrow),
//! Article IV (unknown commands → friendly Error, never silent),
//! Article XIII rule 8 (Action is the typed surface that
//! OutputEvent will wrap when S2 wires the runtime).

pub mod audit;
pub mod checkpoint;
pub mod compact;
pub mod cost;
pub mod help;
pub mod migrate;
pub mod plan;
pub mod rollback;

use std::collections::BTreeMap;

/// Trait every slash-command implements. One impl per file.
///
/// `name()` is the dispatch key (without leading `/`).
/// `description()` shows up in `/help`.
/// `run` receives the post-name arg string + a `CommandContext` with
/// ambient state (currently just a registry handle for `/help` to walk).
pub trait SlashCommand: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn run(&self, ctx: &CommandContext, args: &str) -> CommandOutcome;
}

/// Ambient state passed to every command. Stage 1 carries only a
/// registry handle (so `/help` can list peers). Stage 2+ adds:
/// `Arc<dyn AuditStore>`, `Arc<CheckpointStore>`, current `run_id`,
/// etc. — additive on this struct without touching command signatures.
pub struct CommandContext<'a> {
    pub registry: &'a Registry,
}

/// Result of running a command. The TUI runtime (S2+) pattern-matches:
/// `Text` renders inline, `Action` dispatches the side-effect through
/// the agent loop, `Error` shows red-tinted output (Article IV).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    Text(String),
    Action(Action),
    Error(String),
}

/// Typed side effects. Each variant maps to an `OutputEvent` shape the
/// Stage 2 TUI runtime wraps. Per Article XIII rule 8, adding a variant
/// here MUST be paired with the corresponding `OutputEvent::is_backend_event()`
/// extension when that file lands (Stage 2 / P-14b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    StartMigration { plan_path: String },
    Checkpoint,
    Rollback { run_id: String },
    Compact,
}

/// Slash-command registry. Construct once at TUI init; share via Arc
/// if multiple panels need it.
pub struct Registry {
    commands: BTreeMap<&'static str, Box<dyn SlashCommand>>,
}

impl Registry {
    /// Build the Stage 1 registry — 8 commands. 4 working, 4 stubs.
    /// Per clarify Q1: explicit constructor (no compile-time discovery).
    pub fn stage1() -> Self {
        let mut commands: BTreeMap<&'static str, Box<dyn SlashCommand>> = BTreeMap::new();
        commands.insert("help", Box::new(help::Help));
        commands.insert("audit", Box::new(audit::Audit));
        commands.insert("migrate", Box::new(migrate::Migrate));
        commands.insert("checkpoint", Box::new(checkpoint::Checkpoint));
        commands.insert("plan", Box::new(plan::PlanCmd));
        commands.insert("cost", Box::new(cost::Cost));
        commands.insert("rollback", Box::new(rollback::Rollback));
        commands.insert("compact", Box::new(compact::Compact));
        Self { commands }
    }

    /// How many commands are registered.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// `(name, description)` pairs for rendering. Sorted by `BTreeMap`
    /// iteration order — Article XIII rule 2 cache stability.
    pub fn list(&self) -> Vec<(&'static str, &'static str)> {
        self.commands
            .values()
            .map(|c| (c.name(), c.description()))
            .collect()
    }

    /// Dispatch a raw user input. Accepts `/name args...` or `name args...`
    /// (leading slash optional). Empty input returns `Error`.
    pub fn dispatch(&self, input: &str) -> CommandOutcome {
        let trimmed = input.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            return CommandOutcome::Error("empty command".to_string());
        }
        let (name, args) = match trimmed.split_once(char::is_whitespace) {
            Some((n, a)) => (n, a.trim()),
            None => (trimmed, ""),
        };
        match self.commands.get(name) {
            Some(cmd) => {
                let ctx = CommandContext { registry: self };
                cmd.run(&ctx, args)
            }
            None => CommandOutcome::Error(format!(
                "unknown command '/{}': try /help for available commands",
                name
            )),
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::stage1()
    }
}
