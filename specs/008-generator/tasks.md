# Tasks — P-08

- [ ] T1: Write `libs/engine/src/generator/errors.rs` (`GeneratorError`, `BackupError`)
- [ ] T2: Update `libs/engine/src/mapper/mod.rs` with minimal `MappingPlan`,
      `MappedResource`, `AttributeValue` types (Stage 1 shape; S4 expands)
- [ ] T3: Write `libs/engine/src/generator/backup.rs` (`move_to_backup`,
      `restore_from_backup`, EXDEV copy+remove fallback)
- [ ] T4: Write `libs/engine/src/generator/templates.rs` (10 templates +
      `TemplateRegistry` keyed by `target_type`)
- [ ] T5: Write `libs/engine/src/generator/emitter.rs` (compose Body, write,
      audit-emit)
- [ ] T6: Rewrite `libs/engine/src/generator/mod.rs` with `Generator`,
      `Generator::new`, `with_audit`, `generate`, `rollback`
- [ ] T7: Update `libs/engine/src/lib.rs` to expose generator module
- [ ] T8: Write `libs/engine/tests/generator_test.rs` (8 tests):
      round-trip, backup-first, rollback, template-miss, determinism,
      EXDEV fallback, audit emission, multi-resource MappingPlan
- [ ] T9: `cargo check -p terrashift-engine --tests`
- [ ] T10: `cargo clippy --all-targets -- -D warnings`
- [ ] T11: `cargo fmt -- --check`
- [ ] T12: `cargo test -p terrashift-engine`
- [ ] T13: `cargo test --workspace` (regression sweep)
- [ ] T14: Run `code-reviewer` subagent on staged diff; resolve findings
- [ ] T15: Run `constitution-checker` subagent; resolve findings
- [ ] T16: Write `analyze.md` + `checklist.md` (post-impl docs)
- [ ] T17: Commit `feat(p-08): Generator — deterministic HCL emit + reversible backup`
