# Spec — P-08: Generator (deterministic HCL emit + reversible file ops)

**Stage:** 1 | **P-NN:** P-08 | **Tier:** All
**TERRASHIFT_MAPPING.md:** §A row 2 (ToolExecutor — GeneratorTool impl Tool),
§B row 1 (migration tools — `generate`), §41 Phase 2 (re-skin tool catalog).
**Constitution:** Article I (deterministic-first; Generator is NOT an agent),
Article IV (loud failures on template miss), Article V (reversible file ops),
Article IX (data governance — never auto-delete user files; backups archival),
Article XIII rule 3 (no unwrap/expect/string-slice — HCL emit is high-risk),
Article XIII rule 5 (redaction-on-LLM-fallback path stubbed for S5+).
**Source pattern:** `refs/stakpak_arch.md` §28 (reversible file operations);
`refs/stakpak/libs/shared/src/file_backup_manager.rs:13-70` (move-to-backup).

## Goal

Take a validated `MappingPlan`, produce target `.tf` files in the output
directory. Pure deterministic Rust — NO LLM in happy path. Existing files at
the destination are moved to a backup directory before overwrite, so any
re-run is fully reversible.

## Stage 1 scope

- `Generator` struct + `generate(run_id, plan, output_dir)` API
- Template registry: 10 resource templates covering both Stage 1 demo paths
  - GCP→AWS demo path (terrashift_plan.md §17): `aws_vpc`, `aws_subnet`,
    `aws_security_group`, `aws_instance`, `aws_s3_bucket`
  - AWS→Azure fixture path (pre-flight Decision 10): `azurerm_virtual_network`,
    `azurerm_subnet`, `azurerm_network_security_group`,
    `azurerm_linux_virtual_machine`, `azurerm_storage_account`
- HCL emit via `hcl-rs`'s structured `Body`/`Block` builder API
- Backup-first wrapper: existing files moved to
  `.terrashift/runs/{run_id}/backups/{op_uuid}/<filename>` before overwrite.
  EXDEV (cross-mount) falls back to copy+remove (deliberate deviation from
  Stakpak's hard-bail — see clarify Q4)
- Audit integration: every file write emits `AuditPayload::FileOperation`
  with `backup_path: Some(..)` for overwrites, `None` for greenfield (this
  closes the §28 gap reference-explorer surfaced — Stakpak audits `remove`
  but not `create/overwrite`)
- `MappingPlan` + `MappedResource` types added to `libs/engine/src/mapper/mod.rs`
  (minimal Stage 1 shape; P-05 in S4 expands)
- Round-trip property: emit → re-parse via Scanner is structurally equivalent

## Stage 1 out of scope

- LLM fallback for unmapped resources (Stage 5+). Template miss → loud Err.
  The `Generator` trait surface still defines a `LlmFallback` extension point
  so S5 wiring is purely additive.
- HCL constructs `dynamic` / `count` / `for_each` (Stage 5 — P-27).
- Cross-cloud module rewriting (Stage 5).
- Externalized templates in `terrashift-mappings` repo (Stage 5).
- Real Mapper integration (S4 — uses test-fixture `MappingPlan` for now).
- Real Validator gate between Mapper and Generator (S4 — for P-08 we accept
  any well-formed `MappingPlan`).

## Success criteria

`cargo test -p terrashift-engine` covers:

1. **Round-trip:** emit MappedResource → write `.tf` → re-parse with `hcl::from_str` →
   structurally equivalent (same block label, same attribute set).
2. **Backup-first preservation:** existing `aws_vpc.tf` at output dir is moved to
   `.terrashift/runs/{run_id}/backups/{op_uuid}/aws_vpc.tf` before overwrite;
   the original content is byte-identical to the backup file.
3. **Backup rollback:** `Generator::rollback(run_id)` moves backed-up files back,
   restoring original state. (Article IX — backups are archival, recoverable.)
4. **Template miss is loud:** `MappingPlan` with unknown `target_type` returns
   `Err(GeneratorError::TemplateMiss)` naming the resource. (Article IV.)
5. **Audit emission:** when `AuditStore` is wired, every file write produces
   `AuditPayload::FileOperation` with correct `kind` + `backup_path`.
6. **Determinism:** running the same `MappingPlan` twice produces byte-identical
   output (sorted resources, no embedded times). (Article VI.)
7. **EXDEV fallback:** simulated cross-mount move falls back to copy+remove
   without panicking. (Improves Stakpak's bail-out behavior.)
8. **Clippy clean:** no `unwrap`/`expect`/string-slice in the production tree.
   (Article XIII rule 3 — HCL emit is the next high-risk surface after Scanner.)

All 4 build gates green: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo check --all-targets`, `cargo test --workspace`.
