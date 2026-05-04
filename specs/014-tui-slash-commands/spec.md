# Spec — P-14: TUI slash commands (per-file pattern)

**Stage:** 1 | **P-NN:** P-14 | **Tier:** All
**TERRASHIFT_MAPPING.md:** §F1 (the *one* place we adopt Claude Code's
shape over Stakpak's — Claude Code's `commands/` per-file pattern is
cleaner; documented Stakpak weakness, not stylistic preference).
**Constitution:** Article II (per-file pattern is one of the few
conceptual borrows from Claude Code), Article IV (unknown command →
friendly error, not silent failure), Article XIII rule 8 (don't add new
`InputEvent`/`OutputEvent` variants without updating
`is_backend_event()` — slash-command results travel via these channels).
**Source pattern:** `refs/claude-code/src/commands/<name>/{index.ts,
<name>.ts}` shape — metadata + lazy-loaded handler. Adapted to Rust as
one `.rs` file per command (no lazy-load needed; Rust compiles to one
binary).
**Stakpak counterpart:** `refs/stakpak_arch.md §16` (TUI structure;
AppState + services). Stakpak's slash dispatch is centralised match in
`cli/src/commands/mod.rs` — three places to edit per new command. We
don't adopt that.

## Goal

A `Registry` of slash commands keyed by name. Each command lives in
its own `tui/src/commands/<name>.rs`. The TUI dispatches `/<name> args`
to the registered handler; unknown commands return a friendly error.
Adding a new command = adding one file (Claude Code parity).

## Stage 1 scope

- `SlashCommand` trait: `name`, `description`, `run`.
- `Registry::stage1()` ships **8 commands**:
  - **Working** (deliver real output today):
    - `/help` — list every registered command + description
    - `/audit` — placeholder text describing the audit log location
      (real `verify_chain` integration in S5 when CLI wires
      `LocalAuditStore`)
    - `/migrate <plan_path>` — emits `Action::StartMigration` (handler
      lives in S5+; the dispatch contract is defined now)
    - `/checkpoint` — emits `Action::Checkpoint`
  - **Stubs** (return "Stage 2+: not yet implemented" text — the
    *interface* exists so adding them later is non-breaking):
    - `/plan`, `/cost`, `/rollback`, `/compact`
- `CommandOutcome` enum: `Text` (render to TUI), `Action` (typed
  side-effect for the TUI runtime), `Error` (friendly per Article IV).
- `Action` enum: `StartMigration { plan_path }`, `Checkpoint`,
  `Rollback { run_id }`, `Compact`.
- 1 file per command (~30-50 LOC each); registry-built via explicit
  `Registry::stage1()` constructor (no compile-time directory walk —
  Rust doesn't have a clean equivalent to Claude Code's
  `import.meta.glob`).

## Stage 1 out of scope

- TUI runtime that *consumes* `Action` — Ratatui mainloop arrives in
  Stage 2 / P-14b. Stage 1 just defines the dispatch contract.
- Real `/audit` integration — needs the CLI to wire `LocalAuditStore`
  (S5+).
- Real `/checkpoint` — needs the agent loop kernel (S9 / P-09 family).
- Real `/cost` — needs Infracost integration (S11).
- Real `/compact` — needs `CompactionEngine` impl (S9).

## Success criteria

`cargo test -p terrashift-tui` covers:

1. **Registry has 8 commands** — `Registry::stage1().len() == 8`.
2. **`/help` lists every command** — output text contains every
   registered command name + description.
3. **Unknown command friendly error (Article IV)** — `/nope` returns
   `CommandOutcome::Error(String)` naming the unknown command.
4. **Metadata stability** — every command's `name()` and
   `description()` are non-empty static strings.
5. **Dispatch parses args** — `/migrate path/to/plan.json` produces
   `Action::StartMigration { plan_path: "path/to/plan.json" }`.
6. **Per-file convention** — verified by directory layout (one `.rs`
   per command under `tui/src/commands/`).
7. **Article XIII rule 8** — N/A in Stage 1 (no `InputEvent` variants
   added yet); the registry's `Action` enum is the future-typed
   surface that `OutputEvent` will wrap when S2 wires the runtime.
   Documented inline.

All 4 build gates green.
