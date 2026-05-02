# Checklist — P-08

## Spec Kit gates

- [x] `spec.md` — goal, scope, out-of-scope, success criteria
- [x] `clarify.md` — 10 user-on-behalf decisions documented
- [x] `plan.md` — file table, build order, deps, risk register
- [x] `tasks.md` — T1..T17 atomic task list
- [x] `analyze.md` — post-impl decisions + risks + deviations
- [x] `checklist.md` — this file

## Build gates

- [x] `cargo check -p terrashift-engine --all-targets` — clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — clean
- [x] `cargo fmt --check` — clean
- [x] `cargo test --workspace` — all green; 0 failures across all crates

## Spec success criteria coverage

- [x] #1 round-trip emit→re-parse — `emit_aws_vpc_round_trips_through_scanner`
- [x] #2 backup-first preservation — `backup_first_preserves_original_content`
- [x] #3 backup rollback — `rollback_restores_backed_up_file`
- [x] #4 template miss is loud — `template_miss_is_loud_error`
- [~] #5 audit emission metadata — `EmittedFile` shape verified
      (`greenfield_write_creates_no_backup` + `backup_first_preserves_original_content`).
      End-to-end round-trip through `LocalAuditStore` deferred to S9
      when `AuditWriterHook` from P-11 wires into the agent loop.
- [x] #6 determinism byte-identical — `determinism_byte_identical_across_runs`
- [~] #7 EXDEV fallback — code path written; portable trigger test
      deferred to S5 with real cross-mount environment.
- [x] #8 clippy clean — verified by gate above
- [x] **M1 fix** — `rollback_greenfield_deletes_created_file` + aggregated
      `RollbackPartial` error covers Article V partial-failure invariant
- [x] **M2 fix** — `empty_reference_is_loud_error` locks Article IV behaviour
- [x] **M3 fix** — Test 10 uses TempDir-relative paths (Windows-portable)

## Constitution coverage

- [x] **Article I** — Generator is deterministic-first, NOT an agent
- [x] **Article II** — `stakpak_arch.md §28` cited in every file head;
      `refs/stakpak/libs/shared/src/file_backup_manager.rs:13-70`
      cited as verbatim source for `move_to_backup`
- [x] **Article IV** — every failure mode named in `GeneratorError` /
      `BackupError`; template miss returns Err with the missing type
- [x] **Article V** — every overwrite preserved via backup; reversible
- [x] **Article VI** — sort-by-target_addr + BTreeMap iteration order +
      determinism regression test
- [x] **Article IX** — backups never auto-deleted; `rollback` is the
      explicit restore path
- [x] **Article XIII rule 3** — no `unwrap`/`expect`/string-slice in
      production paths; clippy enforces

## Stakpak deviations documented

- [x] EXDEV fallback (improvement) — `backup.rs` rustdoc + analyze.md §C
- [x] Run-scoped backup tree (improvement) — `backup.rs::backup_root` rustdoc
- [x] Audit on overwrite (gap-fill) — analyze.md §B + §28 gap reference

## Files committed

- [x] `specs/008-generator/{spec,clarify,plan,tasks,analyze,checklist}.md` (6 spec files)
- [x] `libs/engine/Cargo.toml` (added `uuid = { workspace = true }`)
- [x] `libs/engine/src/mapper/mod.rs` (added `MappingPlan`, `MappedResource`, `AttributeValue`, `MapperLookupError`)
- [x] `libs/engine/src/generator/errors.rs` (NEW)
- [x] `libs/engine/src/generator/backup.rs` (NEW)
- [x] `libs/engine/src/generator/templates.rs` (NEW)
- [x] `libs/engine/src/generator/emitter.rs` (NEW)
- [x] `libs/engine/src/generator/mod.rs` (rewrote stub)
- [x] `libs/engine/tests/generator_test.rs` (NEW — 10 tests)

## Reviewer pass

- [ ] `code-reviewer` subagent
- [ ] `constitution-checker` subagent
- [ ] Findings resolved
- [ ] Commit `feat(p-08): Generator — deterministic HCL emit + reversible backup`
