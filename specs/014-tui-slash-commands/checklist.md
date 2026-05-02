# Checklist — P-14

## Spec Kit gates
- [x] spec, clarify, plan, tasks, analyze, checklist (6 files)

## Build gates
- [x] `cargo check -p terrashift-tui --tests` — clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — clean
- [x] `cargo fmt --check` — clean
- [x] `cargo test --workspace` — all green; tui's 12 tests pass

## Spec criteria coverage
- [x] All 7 numbered criteria + 5 bonus invariants covered by tests

## Constitution coverage
- [x] **II** — every command file cites refs/claude-code source + §F1 row
- [x] **IV** — unknown command + missing args produce `Error` with helpful messages
- [x] **VII** — no new Cargo deps; per-file convention reduces edit-sites
- [x] **XIII rule 3** — production paths unwrap/expect/slice-free
- [x] **XIII rule 8** — `Action` enum documented as the typed surface
      `OutputEvent` will wrap when S2 ships the runtime

## Reviewers
- [~] `code-reviewer` agent: quota-limited; self-review covered the
      same checklist. **One MAJOR finding fixed**: Test 8 `matches!`
      bug (vacuous pass without `assert!` wrapper).
- [~] `constitution-checker` agent: quota-limited; self-review covered
      the article matrix. PASS on all 5 articles audited.

## Files committed
- [x] `specs/014-tui-slash-commands/{spec,clarify,plan,tasks,analyze,checklist}.md`
- [x] `tui/src/lib.rs` — exposes `commands` module
- [x] `tui/src/commands/mod.rs` — trait + types + Registry
- [x] `tui/src/commands/{help,audit,migrate,checkpoint,plan,cost,rollback,compact}.rs`
- [x] `tui/tests/slash_commands_test.rs` — 12 integration tests

## SESSION_PLAN ledger
- [x] Row 6b → ✅
