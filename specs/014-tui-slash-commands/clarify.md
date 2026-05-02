# Clarifications — P-14

## Q1: How do we discover commands at runtime?

**Decision:** **Explicit `Registry::stage1()` constructor** that inserts
each command via `Box::new(...)`. No compile-time directory walk.

Why: Rust doesn't have a clean `import.meta.glob` equivalent. We could
build one with a build.rs or proc-macro, but those add complexity
disproportionate to the goal. Explicit list is fine for 8 commands;
revisit when we cross 30+ (Stage 6 plugin system / P-29).

The "one file per command" discipline is preserved because adding a
command is still a single-file edit: write `tui/src/commands/foo.rs`
exporting a `Foo` struct that impls `SlashCommand`, then add one line
to `Registry::stage1()`. The pattern remains "one new file per command."

## Q2: Trait method shapes — `&self`, `&mut self`, async?

**Decision:** **`fn run(&self, args: &str) -> CommandOutcome` — sync, immutable.**

Why sync: Stage 1 commands return `Text` or `Action`; nothing actually
performs IO inside `run`. The TUI runtime (S2+) does the IO based on
the `Action`. Keeping `run` sync also avoids the `async-trait` macro
surface and lets us derive `Send + Sync` cleanly.

When S2+ commands need to await something (e.g., `/audit verify` running
`LocalAuditStore::verify_chain`), promote them to `async` then.
Conversion is mechanical; no surface change for the existing 8.

## Q3: `Action` vs raw `Text` — when does each apply?

**Decision:** Commands that change *state* return `Action(...)`. Commands
that only display info return `Text(String)`. `/help`, `/audit`
placeholder, `/cost` stub, `/rollback` (S5+) all render text. `/migrate`,
`/checkpoint`, `/compact` cause side effects → `Action(...)`.

The TUI runtime (S2+) pattern-matches on `CommandOutcome` and either
prints text or dispatches the action.

## Q4: How does `/help` discover descriptions without circular reference?

**Decision:** `Registry::list()` returns `Vec<(name, description)>`
borrowed from the registered commands. `/help`'s handler in
`commands/help.rs` accepts a `&Registry` (passed by the dispatcher) so
it can read the list.

Slight wrinkle: this means `Registry::dispatch` needs to pass `self` to
`Help::run`. We solve it by giving `SlashCommand::run` a
`&Registry` context (`fn run(&self, ctx: &CommandContext, args: &str)`)
where `CommandContext` carries a `&Registry` reference + (S2+) other
ambient state.

## Q5: Hidden commands? Aliases? Subcommands?

**Decision:** Stage 1 ships **none of these**. All 8 commands visible.
No aliases. No subcommands.

Stage 5+ may add: `/audit verify`, `/audit export` (subcommands),
`/h` alias for `/help` (alias map). The trait surface accommodates
both via the `args: &str` parameter; we just don't parse them yet.

## Q6: Argument parsing — clap-style or custom?

**Decision:** **Custom; `args: &str` is the entire post-name string.**
Each command parses what it needs.

Why not clap: clap's `--flag` model is heavy for slash commands which
typically take 0-1 positional args. `/migrate <plan_path>` parses
`args.trim()` as the path; `/audit` ignores args entirely.

## Q7: Per-command file size limit?

**Decision:** Soft target ≤ 50 LOC per command file. Registry + trait +
context types live in `tui/src/commands/mod.rs` (~120 LOC).
