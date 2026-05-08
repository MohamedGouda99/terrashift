# Tasks — r07-mvp-closure

Atomic, dependency-ordered. Each task ≤30 lines of code.

| # | Task | File | Depends |
|---|---|---|---|
| T1 | Add `TemplateRegistry::registered_types()` returning iterator over registered keys | `libs/engine/src/generator/templates.rs` | — |
| T2 | Add `MapperError::{EmptyTargetType, UnsupportedTargetType, Knowledge}` variants | `libs/engine/src/mapper/errors.rs` | — |
| T3 | Bump SYSTEM_PROMPT version `v1 → v2` + extract header into const | `libs/engine/src/mapper/prompt.rs` | — |
| T4 | Add `build_system_prompt(target_provider, target_schema, supported_types)` builder | `libs/engine/src/mapper/prompt.rs` | T3 |
| T5 | Add `Mapper::supported_types_for(target_provider)` helper | `libs/engine/src/mapper/mod.rs` | T1 |
| T6 | Add `Mapper::validate_plan(plan, supported_types)` validation pass | `libs/engine/src/mapper/mod.rs` | T2, T5 |
| T7 | Wire schema lookup + new prompt builder into `Mapper::map` | `libs/engine/src/mapper/mod.rs` | T4, T6 |
| T8 | Unit tests for new validation: empty/unsupported/valid target_type | `libs/engine/src/mapper/mod.rs` | T7 |
| T9 | Unit tests for prompt builder: with-schema / without-schema fallback | `libs/engine/src/mapper/prompt.rs` | T4 |
| T10 | Add `default_output_for(source, target)` helper in migrate.rs | `cli/src/commands/migrate.rs` | — |
| T11 | Replace tempdir fallback with `default_output_for` in migrate.rs | `cli/src/commands/migrate.rs` | T10 |
| T12 | Unit tests for `default_output_for` covering 5 path shapes | `cli/src/commands/migrate.rs` | T10 |
| T13 | `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` clean | (workspace) | T1-T12 |
| T14 | `cargo test --workspace` clean | (workspace) | T13 |
| T15 | Manual e2e: rebuild binary, run on `fixtures/e2e-aws-to-azure/`, ≥4/5 emit | (fixture) | T14 |
| T16 | Update `SESSION_PLAN.md` to record this session as `S8r07` remediation | `docs/governance/SESSION_PLAN.md` | T15 |
| T17 | Commit + push branch + open PR | (git) | T16 |
