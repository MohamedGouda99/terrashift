# Clarifications — P-08

User-on-behalf decisions per CLAUDE.md / constitution / SESSION_PLAN.md S3b.
Surface any disagreement before implement; otherwise these are committed.

## Q1: Which resource templates to prioritize for Stage 1 (5–10)?

**Decision:** **10 templates split across both Stage 1 demo paths.**

| Direction | Templates |
|---|---|
| GCP→AWS (terrashift_plan.md §17 demo) | `aws_vpc`, `aws_subnet`, `aws_security_group`, `aws_instance`, `aws_s3_bucket` |
| AWS→Azure (pre-flight Decision 10 Pratik fixture) | `azurerm_virtual_network`, `azurerm_subnet`, `azurerm_network_security_group`, `azurerm_linux_virtual_machine`, `azurerm_storage_account` |

**Why both:** the §17 demo lives in GCP→AWS; the Pratik fixture lives in
AWS→Azure. Stage 1 needs to demo *and* eval against goldens. Five each is
the smallest set that exercises VPC + compute + storage + IAM-adjacent
patterns end-to-end. Each template is ~25 LOC, total ~250 LOC.

## Q2: HCL emit — `hcl-rs` structured API or string templates?

**Decision:** **`hcl-rs`'s structured `Body`/`Block` builder API.**

Reason: string templates are an Article XIII rule 3 magnet (`format!` with
unescaped substitutions is the same shape as `&s[..n]`). The structured API
handles quoting, expression vs literal, and traversal-attribute references
safely. Trade-off: slightly more verbose per template; pays off in zero
escaping bugs.

Reference: `hcl-rs` 0.18's `hcl::Body` + `hcl::Block::builder()` pattern.

## Q3: Backup directory scheme?

**Decision:** **`.terrashift/runs/{run_id}/backups/{op_uuid}/<filename>`.**

Improves on Stakpak's `.stakpak/session/backups/{uuid}/` (per
`refs/stakpak/libs/shared/src/file_backup_manager.rs:21`) by adding the
`run_id` layer. Audit replay scopes by `run_id`; orphan backups without a
run scope would be a bug (Article IV). The `op_uuid` per file write
preserves Stakpak's per-call isolation guarantee.

## Q4: EXDEV (cross-mount) fallback behavior?

**Decision:** **`std::fs::rename` first; on `ErrorKind::CrossesDevices`
fall back to copy+`fs::remove_file`.** Document as a deliberate deviation
from Stakpak's bail-out (`file_backup_manager.rs:35-41` returns the EXDEV
error verbatim).

Why deviate: Article IV says fail loudly, but EXDEV is a *recoverable*
error. Bailing out turns a backup into a `BACKUP_ERROR` in cases where
the user's `output_dir` happens to be on a different mount than `cwd` —
common on Windows with separate drives, on Linux with `/tmp` on tmpfs.
Copy+remove preserves the backup invariant and is loud only when the
*real* errors happen (permissions, disk full).

## Q5: Template-miss behavior — loud Err or LLM fallback?

**Decision:** **Stage 1: loud `Err(GeneratorError::TemplateMiss)`** naming
the unknown `target_type`. NO LLM call.

The P-08 prompt allows LLM fallback "if the resource is in scope (rare in
Stage 1)." We simplify by deferring the entire LLM-fallback path to S5
(P-27 — HCL constructs). The `Generator` trait surface defines
`fn template_for(&self, target_type: &str) -> Option<TemplateFn>` so S5
wiring is purely additive (a new `LlmFallbackGenerator` impl wrapping the
deterministic one).

This also defers the Article XIII rule 5 redaction-on-LLM-fallback
implementation. The path is documented in the spec as Stage 5+ scope.

## Q6: Output file layout — one file per resource type, or grouped?

**Decision:** **One file per `target_type`** (e.g., `aws_vpc.tf`,
`aws_subnet.tf`).

Stage 5 may switch to logical grouping (`network.tf`, `compute.tf`) if
golden migrations show it improves diff readability. Stage 1's bias is
toward simplicity — one file per type is unambiguous, easy to diff, easy
to round-trip-test.

## Q7: Where does `MappingPlan` live?

**Decision:** **`libs/engine/src/mapper/mod.rs`.**

Stage 1: minimal type definition (Mapper is its source, Generator consumes).
P-05 in S4 expands the type; Generator imports stay stable as long as
field additions are non-breaking. Don't put it in `libs/shared` — it's an
engine-internal contract, not a cross-crate boundary.

## Q8: Determinism — what's the byte-stability mechanism?

**Decision:** **Three-part discipline:**

1. Sort `MappedResource` entries by `target_addr` before emit (input order
   from Mapper is not guaranteed stable).
2. NO embedded times, UUIDs, or random numbers in emitted HCL.
3. `hcl::to_string(&body)?` is presumed deterministic; verified by a
   regression test that re-emits the same fixture twice and asserts
   byte-equality. If hcl-rs ever changes formatting, the test catches it.

Article VI ("same migration today gives same output six months from now")
is enforced by this regression test — not reviewer vigilance.

## Q9: Audit integration shape?

**Decision:** **`Generator::with_audit(Arc<dyn AuditStore>)` builder.**

Constructor: `Generator::new()` returns a Generator with no audit (used
for unit tests where audit is out of scope). `with_audit(store)` adds
audit emission. Every file write inside `generate()` calls
`audit.append(AuditEntry { payload: FileOperation { ... } })`.

Why builder, not constructor parameter: keeps the unit tests fast and
avoids forcing audit setup in every test fixture. Integration tests
explicitly opt in via `with_audit`.

The `AuditWriterHook` from P-11 is a different mechanism (it captures
*tool execution* via the agent loop, which arrives in S9). Generator's
direct `audit.append` covers Stage 1 where there is no agent loop yet.

## Q10: Error type — `thiserror` enum vs `anyhow`?

**Decision:** **`thiserror`-derived `GeneratorError` enum.** Consistent
with Scanner (P-04) and Audit (P-11). Variants:
`TemplateMiss { target_type }`, `Hcl(hcl::Error)`, `Io(io::Error)`,
`Backup(BackupError)`, `Audit(AuditError)`.
