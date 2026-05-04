# Clarifications — P-12

User-on-behalf decisions per CLAUDE.md / SESSION_PLAN.md S3b. Surface any
disagreement before implement; otherwise these are committed.

## Q1: Where do golden migrations live?

**Decision:** **`terrashift-evals/` at the workspace root, committed
to git.**

SESSION_PLAN.md lists `terrashift-evals` as a private repo (Stage 6+
SaaS) but in Stage 1 we ship 3 small first-party fixtures alongside the
code. They're tiny (~200 LOC of HCL each), Apache-2-compatible (we
hand-author them), and their drift detection IS the value of P-12. When
we cross 10 goldens (S7) and they include real customer-derived shapes,
move them to a private repo per the original SESSION_PLAN intent.

Pre-flight Decision 10 (PratikMahajan fixture at `fixtures/aws-to-azure-real/`)
is GPL-2 and gitignored; that's a separate concern. Our 3 hand-curated S3b
goldens are first-party.

## Q2: Comparison strategy — byte-equality or `insta` snapshots?

**Decision:** **Byte-equality.** Defer `insta` to S7 expansion.

Reference-explorer flagged `insta` as the more ergonomic option (better
diffs, `cargo insta review` workflow). Trade-off: adds tooling complexity
in S3b when we have 3 goldens that fit on one screen. Generator's P-08
determinism regression test (`determinism_byte_identical_across_runs`)
already proves byte-stability, so byte-equality won't false-positive on
unstable formatting.

Switch criterion: when goldens grow past ~10 (S7) and human review of
diffs becomes painful, port to `insta`. Revisit clarify Q2 then.

## Q3: Manifest format — TOML vs JSON?

**Decision:** **TOML.** Matches the existing config convention
(`~/.terrashift/config.toml` per terrashift_plan.md §6.X) and is more
human-editable for the per-golden manifest fields. JSON is reserved for
machine-emitted data (`mapping_plan.json`).

## Q4: `MappingPlan` fixture format — JSON or Rust constructors in tests?

**Decision:** **`mapping_plan.json` per golden** (committed alongside
source.tf and expected/ directory).

Rationale: when S4 wires the real Mapper, the same goldens become input
fixtures for Mapper-output regression tests. Pre-curating as JSON gives
that future work a stable contract. Mapper's output is JSON
(per-terrashift_plan.md §6.X "Single LLM call producing strict JSON
conforming to schemars-derived MappingPlan schema"), so the format choice
also forces us to verify our `MappingPlan` Serialize/Deserialize impls
work end-to-end.

## Q5: `EvalResult` token cost — placeholder or omit entirely?

**Decision:** **Placeholder field with `0` default.** Plumbing exists from
day one even though Stage 1 doesn't populate it.

Why: when S4 lands real LLM calls, we don't want the framework's
public types to change shape — that would break every downstream consumer
(CI scripts, dashboards). Defining `token_cost_micros: u64` as a field now
keeps the API stable. `manifest.toml`'s `token_cost_ceiling_micros` field
also exists from day one for the same reason.

## Q6: CI integration — feature-gated like Stakpak's `ci.yml:38-43`?

**Decision:** **Stage 1: NO feature-gating** — goldens run inside
`cargo test --workspace`. **S7: ADD feature-gating** when goldens start
exercising LLMs (slow, paid).

Stakpak gates because their `libsql-test` and `network-tests` features
are slow/external. Our 3 S3b goldens are pure deterministic local
filesystem ops — they finish in milliseconds and don't talk to the
network. Feature-gating now adds zero value and one more thing to forget.

## Q7: Diff format — unified diff or pretty side-by-side?

**Decision:** **Unified diff via the `similar` crate** when one is
present. Output stored in `EvalResult.diff: Option<String>`.

Why `similar`: small dep, single purpose; simple unified-diff API;
outputs human-readable plain text suitable for CI logs. Alternative
considered: hand-rolled byte-by-byte diff — rejected, unreadable for HCL.

## Q8: Runner shape — instance method or free function?

**Decision:** **Method `EvalRunner::run(&self, golden)`** on a thin
struct that bundles a `Generator`.

Pattern matches Scanner from P-04 (`Scanner::scan(root)` is static-style
on a stateless struct). Generator is the only stateful dependency, and
we already build it once per process. The struct also gives us a place
to thread a future `LocalAuditStore` reference in S9.

## Q9: Hermeticism — how do we keep concurrent test runs from colliding?

**Decision:** **`tempfile::TempDir` per fixture run.** Both `cwd` (for
backup tree) and `output_dir` (for emitted .tf) are fresh tempdirs.

Generator's `generate(cwd, output_dir, plan)` already accepts these as
parameters specifically to enable this isolation (see clarify Q9 in
specs/008-generator/clarify.md). The runner inherits the discipline.

## Q10: Where does `insta` defer hint live so we remember in S7?

**Decision:** **A `// TODO(S7): switch to insta` comment at the top of
`scorer.rs`** + this clarify.md entry. SESSION_PLAN.md row 7 already
mentions "10 golden migrations" — implicit pointer.
