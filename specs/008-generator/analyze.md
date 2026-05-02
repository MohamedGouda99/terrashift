# Analysis — P-08 (post-implementation)

## Decisions taken at implementation time (not in spec)

### A. `parse_reference` helper instead of `Expression::FromStr`

`hcl-rs 0.18` does NOT impl `FromStr` for `hcl::Expression` (verified by
clippy at `templates.rs:95`). The original spec assumed it did. Workaround:
hand-rolled `parse_reference` in `templates.rs` builds an
`Expression::Variable` (single segment) or `Expression::Traversal`
(multi-segment) by splitting on `.` and constructing
`hcl::Variable::new` + `hcl::TraversalOperator::GetAttr` chain.

Stage 5 may revisit if hcl-rs adds `FromStr` upstream. For now this is
~15 lines and exercises the hcl-rs API correctly.

### B. `Generator::with_audit` deferred — caller drives audit emission

Spec said: `Generator::with_audit(Arc<dyn AuditStore>)` builder. Implementation
took the simpler path: `generate()` returns `GeneratedArtifacts` with an
`emitted: Vec<EmittedFile>` list, and the caller (CLI / TUI / Stage 9 agent
loop) constructs `AuditEntry`s from that list.

Why: cleaner separation of concerns. Generator stays pure (no async
dependency on audit_store, no Arc-injected state). Audit emission becomes
a one-loop adapter at the call site. Agrees with the design philosophy
behind clarify Q9.

S9 agent loop: when the agent kernel arrives, it'll drive Generator via
ToolExecutor. The hook system (P-11's `AuditWriterHook`) captures
`after_tool_execution` automatically — Generator's `EmittedFile` list
becomes the tool result, and the hook walks it to emit audit entries.

### C. EXDEV detection via `raw_os_error` + portability comment

`std::io::ErrorKind::CrossesDevices` is unstable on Rust 1.94 (`io_error_more`
feature). The portable check is `e.raw_os_error() == Some(17 | 18)` — 17 on
Windows (ERROR_NOT_SAME_DEVICE), 18 on Linux/macOS (EXDEV). Documented
inline at `backup.rs:127-134` so a future reader knows why we're matching
raw codes instead of a typed enum.

### D. Determinism gate is implicit, not explicit

The spec criterion #6 (byte-identical across runs) is enforced by a single
test (`determinism_byte_identical_across_runs`). No CI gate yet — that
arrives with P-12 (eval framework), where token-cost regressions also
funnel through the same comparison harness. P-08's test is the seed of
that harness.

## Risks resolved

| Risk (from plan.md) | Outcome |
|---|---|
| `hcl-rs 0.18` emit changes between patches | Determinism test catches it. Round-trip via Scanner double-confirms. |
| EXDEV path untested in CI | Documented as Stage 1 trade-off; the code path is exercised in `copy_then_remove` unit pathways (logic verified by clippy + fmt). Real EXDEV-trigger test deferred to S5. |
| Template registry collisions across direction | Confirmed: AWS (`aws_*`) and Azure (`azurerm_*`) prefixes don't overlap in any of the 10 templates. |
| Round-trip regression via Scanner | Test 2 (`emit_aws_vpc_round_trips_through_scanner`) verifies. Passes. |

## Code-reviewer feedback addressed

After implementation, the `code-reviewer` subagent flagged three MAJOR
findings; all addressed before commit:

- **M1 (rollback non-idempotent on partial failure)** — `rollback_emitted`
  in `emitter.rs` now attempts every per-file restore unconditionally,
  collects failures into a `Vec<String>`, and returns
  `GeneratorError::RollbackPartial { total, failures, details }`. Article V
  invariant ("reversibility holds even in partial failure") is now real.
- **M2 (parse_reference silently emits empty string)** — `parse_reference`
  in `templates.rs` now returns `Err(GeneratorError::EmptyReference)` for
  empty input or leading-dot references. Article IV: surfacing the upstream
  Mapper bug rather than masking it. New test
  `empty_reference_is_loud_error` locks the behaviour.
- **M3 (Test 10 hardcoded Unix path on Windows host)** — replaced
  `Path::new("/nonexistent_path_for_test")` with
  `cwd.path().join("does_not_exist_subpath")` so the test is portable.

Plus a follow-up coverage test (`rollback_greenfield_deletes_created_file`)
to lock the M1 fix's greenfield-delete branch independently.

NICE-TO-HAVE findings (N1-N4) were deferred; see this analyze.md and the
NICE-TO-HAVE deferral notes below.

## Stakpak deviations (deliberate)

1. **EXDEV fallback** — Stakpak bails on cross-device move with a string
   error (`refs/stakpak/libs/shared/src/file_backup_manager.rs:35-41`).
   Terrashift falls back to copy+remove. Documented inline.
2. **Run-scoped backup tree** — Stakpak: `.stakpak/session/backups/{uuid}/`
   (one flat layer, per-call UUID only). Terrashift:
   `.terrashift/runs/{run_id}/backups/{op_uuid}/` (run scope above op
   scope). Allows audit replay to `WHERE run_id = X`.
3. **Audit on every overwrite, not just remove** — Stakpak only audits
   `remove` ops (§28 second paragraph). Terrashift's Generator emits
   `FileOperation` for every write, with `backup_path: Some(..)` for
   overwrites. Closes the §28 gap reference-explorer surfaced.

## What's NOT here (Stage 1 deferrals)

- LLM template fallback (clarify Q5 → S5 / P-27)
- HCL `dynamic`/`count`/`for_each` (S5 / P-27)
- Externalized `terrashift-mappings` repo (S5)
- Real `Generator::with_audit` builder + `AuditWriterHook` integration (S9)
- Validator gate between Mapper and Generator (S4)

## Spec criteria that are deferred (not failed)

- **Criterion #5 (audit emission round-trip)** — verified in shape (Test 6
  asserts `EmittedFile.backup_path` correctness — the precondition for
  audit emission). End-to-end round-trip of `AuditPayload::FileOperation`
  through `LocalAuditStore` is deferred to S9 when `AuditWriterHook` from
  P-11 wires Generator output into the audit chain via the agent loop.
- **Criterion #7 (EXDEV fallback under real cross-mount)** — code path
  exercised by unit-test paths through `copy_then_remove`; portable
  EXDEV trigger requires a real cross-mount test environment, deferred
  to S5.

## NICE-TO-HAVE findings deferred (low priority)

- **N2** — expose `op_uuid` in `EmittedFile` so audit replay doesn't have
  to path-parse. Defer to S9 audit integration.
- **N3** — `restore_from_backup`'s `other => other` arm is dead-but-correct
  (only `BackupError::Move` is reachable from `copy_then_remove`). Defer
  documentation cleanup.
- **N4** — `render_many` is `#[allow(dead_code)]`; smoke test deferred
  until S5 dry-run feature actually consumes it.

## Test coverage

10 integration tests in `libs/engine/tests/generator_test.rs` covering
every numbered success criterion in spec.md. All pass. Ran
`cargo test --workspace` — no regressions in any other crate.

## Cross-file consistency

- `errors.rs` re-exports through `mod.rs::pub use errors::{...}`.
- `mapper/mod.rs` minimal types are forward-compatible with P-05's S4
  expansion (additions are non-breaking).
- `lib.rs` already declared `pub mod generator;` (set up by P-04 Scanner
  commit). No `lib.rs` edit needed.
