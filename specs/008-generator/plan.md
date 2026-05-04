# Plan — P-08

## Files

| File | Purpose | Lines (est) |
|---|---|---|
| `libs/engine/src/mapper/mod.rs` | UPDATE: define `MappingPlan` + `MappedResource` minimal types | ~50 |
| `libs/engine/src/generator/mod.rs` | UPDATE: `Generator` struct + `generate()` + `with_audit()` + `rollback()` | ~150 |
| `libs/engine/src/generator/templates.rs` | NEW: 10 resource template fns + `TemplateRegistry` | ~250 |
| `libs/engine/src/generator/emitter.rs` | NEW: compose `Vec<Block>` → `hcl::Body` → `to_string` → write | ~80 |
| `libs/engine/src/generator/backup.rs` | NEW: `move_to_backup` + `restore_from_backup` (Stakpak `.backup` mirror + EXDEV fallback) | ~100 |
| `libs/engine/src/generator/errors.rs` | NEW: `GeneratorError` + `BackupError` enums | ~40 |
| `libs/engine/tests/generator_test.rs` | NEW: round-trip + backup-first + rollback + template-miss + determinism + EXDEV + audit tests | ~250 |

**Total est:** ~920 lines (production ~620 + tests ~250 + types ~50). Slightly
above Article VII's 500-line preference but within reasonable per-PR review
scope. Same scale as P-04 Scanner (which shipped ~1180 LOC in one PR).

## Build order

1. `errors.rs` (no deps)
2. `mapper/mod.rs` types (`MappingPlan`, `MappedResource`) — Stage 1 minimal
3. `backup.rs` (deps: `errors`)
4. `templates.rs` (deps: `errors`, `mapper::MappedResource`, `hcl-rs`)
5. `emitter.rs` (deps: `templates`, `errors`)
6. `mod.rs` rewrite — `Generator::new` / `with_audit` / `generate` / `rollback`
7. Tests
8. `cargo check -p terrashift-engine --tests`
9. `cargo clippy --all-targets -- -D warnings`
10. `cargo fmt -- --check`
11. `cargo test -p terrashift-engine`
12. `cargo test --workspace` (regression check)
13. `code-reviewer` agent → resolve findings
14. `constitution-checker` agent → resolve findings
15. Commit

## Cargo dep updates

`libs/engine/Cargo.toml` already has all deps needed (`hcl-rs`, `walkdir`,
`tokio`, `serde`, `terrashift-audit`, `tempfile` dev-dep). One addition:

- `uuid = { workspace = true }` — for `op_uuid` and `run_id` types in
  `Generator::generate(run_id: Uuid, ...)`. Already a workspace dep.

No new external crates required. ✓ stays within Article XII rule 1's
"every component has documented per-tier cost ceiling" — this component is
deterministic so cost ceiling is `0 tokens/migration`.

## Backup directory layout

```
.terrashift/
└── runs/
    └── {run_id}/
        ├── backups/
        │   ├── {op_uuid_1}/
        │   │   └── aws_vpc.tf       # the original, moved here before overwrite
        │   └── {op_uuid_2}/
        │       └── aws_subnet.tf
        └── (future: artifacts/, state/, ...)
```

Cleanup policy: **never auto-delete** (Article IX). User runs
`terrashift cleanup --run-id <id>` (Stage 6 CLI surface) when ready.

## Audit emission

Within `generate()` for each file write:

```rust
if let Some(audit) = &self.audit {
    let entry = AuditEntry::new(
        run_id,
        Actor::System,
        "generator.write_file",
        Outcome::Ok,
        AuditPayload::FileOperation {
            path: target_path.clone(),
            kind: if had_existing { FileOpKind::Modify } else { FileOpKind::Create },
            backup_path: backup_path_opt,
        },
    );
    audit.append(entry).await?;
}
```

The audit store is the `LocalAuditStore` from P-11 — already shipped in
commit `4545065`. No new audit code needed.

## Citation pattern (every file head)

```rust
//! Pattern: stakpak_arch.md §28 (reversible file operations).
//! Source: refs/stakpak/libs/shared/src/file_backup_manager.rs:13-70
//!         (verbatim move-to-backup; we add EXDEV copy+remove fallback).
//! Constitution: Article I (deterministic), IV (loud failures),
//!               V (reversible), IX (archival), XIII rule 3 (no unwrap).
```

## Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| `hcl-rs` 0.18 emit changes between patches | Low | Determinism regression test catches it |
| EXDEV path untested in CI | Medium | Use `tempfile` in `/tmp` + write a marker file in `target/` (cross-mount on most CI hosts) |
| Template registry collisions across direction | Low | Templates keyed by `target_type` string; AWS and Azure types don't overlap |
| Round-trip regression because Scanner can't handle generated output | Medium | Test wires Generator output back through Scanner from P-04 |
