# Plan — P-14

## Files (production)

| File | Purpose | Lines |
|---|---|---|
| `tui/src/lib.rs` | UPDATE — re-export `commands` module | ~5 |
| `tui/src/commands/mod.rs` | NEW — `SlashCommand` trait, `CommandOutcome`, `Action`, `Registry`, `CommandContext` | ~150 |
| `tui/src/commands/help.rs` | NEW — `/help`; lists registry contents | ~30 |
| `tui/src/commands/audit.rs` | NEW — `/audit`; placeholder text (real wiring S5) | ~25 |
| `tui/src/commands/migrate.rs` | NEW — `/migrate <path>`; emits `Action::StartMigration` | ~35 |
| `tui/src/commands/checkpoint.rs` | NEW — `/checkpoint`; emits `Action::Checkpoint` | ~25 |
| `tui/src/commands/plan.rs` | NEW — `/plan` stub | ~25 |
| `tui/src/commands/cost.rs` | NEW — `/cost` stub | ~25 |
| `tui/src/commands/rollback.rs` | NEW — `/rollback`; emits `Action::Rollback` | ~30 |
| `tui/src/commands/compact.rs` | NEW — `/compact`; emits `Action::Compact` | ~25 |

## Tests

| File | Coverage |
|---|---|
| `tui/tests/slash_commands_test.rs` | Registry dispatch, `/help` listing, unknown command error, per-command metadata, arg parsing |

**Total est:** ~625 lines (375 production + 250 tests). Within Article VII.

## Build order

1. `commands/mod.rs` — trait + types + registry shell
2. `commands/help.rs`, `audit.rs`, `migrate.rs`, `checkpoint.rs` — working
3. `commands/plan.rs`, `cost.rs`, `rollback.rs`, `compact.rs` — stubs
4. `lib.rs` re-export
5. Integration test
6. cargo gates → reviewers → commit

## Cargo dep updates

`tui/Cargo.toml` already has all needed deps. **No changes.**

## Citation pattern (every command file head)

```rust
//! `/<name>` slash command.
//!
//! Pattern: refs/claude-code/src/commands/<corresponding_name>/index.ts
//!         (per-file metadata + handler). Per TERRASHIFT_MAPPING.md §F1,
//!         this is one of the few patterns we adopt from Claude Code
//!         over Stakpak — Stakpak centralises slash-dispatch in
//!         cli/src/commands/mod.rs (3 edit-sites per command); we keep
//!         it to one file per command.
//! Constitution: Article II (Claude Code borrow), Article IV (friendly
//!               errors on bad args).
```
