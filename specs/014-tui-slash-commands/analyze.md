# Analysis — P-14 (post-implementation)

## Decisions taken at implementation time

### A. `matches!` macro bug in test 8 (caught by self-review)

Tests 8a/8b (`checkpoint_emits_action`, `compact_emits_action`)
originally used `matches!(...)` bare — the macro returns `bool` but
without `assert!` wrapping it the result is silently discarded and the
test passes vacuously. Self-review caught this before commit; both
tests now use `assert!(matches!(...))`. Inline comment in the test
file documents the pitfall to prevent repeat.

### B. Stub-vs-Action split (`/plan`/`/cost` text vs `/rollback`/`/compact` Action)

`/rollback` and `/compact` emit typed `Action` variants because their
side-effects map to operations Stage 1 already has machinery for
(`Generator::rollback` from P-08; `CompactionEngine` from P-02). The
`Action` exists; only the runtime that dispatches it is deferred.

`/plan` and `/cost` return Text describing deferral because their
operations are diffuse — `/plan` needs the plan-mode lifecycle UI, `/cost`
needs the Infracost service crate. Defining bare `Action::Plan` and
`Action::Cost` variants now would lock semantics that Stage 2's design
might want to revise.

This split is intentional, not inconsistent.

### C. Reviewer agents quota-limited; self-review used checklist

Both `code-reviewer` and `constitution-checker` agents hit the daily
usage quota mid-session. Self-review walked the same prompt checklists
that I gave the agents (architecture-correctness, per-file fidelity,
Article II/IV/VII/XIII coverage). Findings:

- **MAJOR fixed**: Test 8 `matches!` bug (above).
- **NICE-TO-HAVE**: stub-split documentation — added inline comment
  in `commands/mod.rs` near the `Action` enum.
- **Article II**: every file head cites
  `refs/claude-code/src/commands/<corresponding>/index.ts` plus
  TERRASHIFT_MAPPING.md §F1.
- **Article XIII rule 3**: `tui/src/commands/` grep for `unwrap()` /
  `expect(` / `&s[..n]` returns 0 hits. Clean.

## Stakpak / Claude Code reference fidelity

Per-file pattern faithfully ported from
`refs/claude-code/src/commands/help/{index.ts, help.tsx}`:
- Claude Code splits metadata (`index.ts`) from handler (`<name>.tsx`)
  for lazy-loading benefit.
- Rust compiles to one binary; lazy-load isn't a concern. We collapse
  metadata + handler into a single `<name>.rs` per command.
- The "one new file per new command" property is preserved — the
  *visible* deviation from Claude Code (file count per command) is
  driven by language semantics, not pattern divergence.

The Stakpak alternative (centralised match in `cli/src/commands/mod.rs`,
3 edit-sites per new command) is documented as the explicit thing we're
NOT adopting per `TERRASHIFT_MAPPING.md §F1`.

## What's NOT here (Stage 1 deferrals)

- **TUI runtime mainloop** — Ratatui `App::run()` consuming
  `CommandOutcome` arrives in Stage 2 / P-14b.
- **Real `/audit verify` integration** — needs `Arc<dyn AuditStore>` in
  `CommandContext`. Stage 5 when CLI wires the store.
- **Real `/checkpoint` envelope persistence** — needs the agent loop
  kernel (S9 / P-09 family). The `Action::Checkpoint` variant exists;
  S9 wires the consumer.
- **Real `/cost`** — Infracost service crate (S11).
- **Real `/compact`** — `CompactionEngine` impl (S9).
- **Subcommands + aliases** (clarify Q5) — Stage 5+.
- **Plugin system** (clarify Q1) — Stage 6 / P-29.

## Spec criteria coverage

| Criterion | Test |
|---|---|
| #1 8 commands | `registry_has_eight_commands` |
| #2 /help lists all | `help_lists_every_command` |
| #3 unknown command friendly error | `unknown_command_returns_friendly_error` |
| #4 metadata stability | `every_command_has_nonempty_metadata` |
| #5 dispatch parses args | `migrate_parses_path_argument` + `migrate_without_path_errors` + `rollback_parses_run_id` |
| #6 per-file convention | verified by directory layout (8 .rs files, one per command) |
| #7 Article XIII rule 8 forward-compat | inline doc near `Action` enum in `commands/mod.rs` |

Bonus tests: empty input handling, leading-slash optional, action emission for checkpoint/compact, stub text for plan/cost.
