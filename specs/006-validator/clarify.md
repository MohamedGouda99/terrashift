# Clarifications — P-06

User-on-behalf decisions per CLAUDE.md / SESSION_PLAN.md S4a.

## Q1: How does Validator get the target schema?

**Decision:** **Constructor injection of `Arc<KnowledgeService>`.**

Pattern matches Generator's `Arc<dyn AuditStore>` shape (P-08). The
Validator calls `knowledge.fetch_schema(target_provider, target_version)`
once per `validate()` call. Tests construct a test-only KnowledgeService
backed by `StubSchemaFetcher` (already in P-07).

## Q2: How does Validator know the target provider version?

**Decision:** **Take `target_version: &str` as a `validate()` parameter,
not a field on `MappingPlan`.**

Why not extend `MappingPlan`: changing its serde shape would invalidate
the goldens' `mapping_plan.json` files committed in P-12. Forward-compat
is preserved by passing the version at the call site.

The eval framework already has access to versions via
`GoldenManifest.target_provider` + per-fixture metadata. S4 (P-05) Mapper
will read the user's `terraform.required_providers` block and pass the
pinned version to Validator.

## Q3: Errors blocking, warnings non-blocking — what's the boundary?

**Decision:** **Three blocking errors (UnknownResourceType,
UnknownAttribute, MissingRequiredAttribute), two non-blocking warnings
(DeprecatedAttribute, SetComputedAttribute).**

Boundary rationale:
- **Blocking**: anything that means `terraform plan` will fail. Hallucinations
  and missing requireds are deterministic terraform errors.
- **Warning**: anything `terraform plan` accepts but a careful operator
  wouldn't ship. Deprecated attrs work today but break later; setting
  computed attrs is permitted (terraform ignores) but indicates the
  Mapper got confused.

Stage 5 may upgrade some warnings to errors based on user feedback.

## Q4: Do we type-check attributes in Stage 1?

**Decision:** **No type checking in Stage 1** (defer to S5 / P-27).

Why: `AttributeSchema.attribute_type` is an HCL type expression string
("string", "list(string)", "map(any)", "object({a: string, b: number})").
Parsing it correctly requires an HCL-type-system implementation. That's
a substantial engineering effort (~500 LOC + tests). Stage 1 ships
without it; the existence-check (UnknownAttribute) catches the most
common Mapper mistakes anyway.

If a Mapper emits `cidr_block: 42` (number where string expected),
`terraform plan` catches it cleanly at Executor time (P-09). Worst case:
one extra round trip through the user feedback loop. Acceptable for Stage 1.

## Q5: How do we test the Article III invariant ("Validator on the path")?

**Decision:** **Two tests: (a) a unit test against
`Validator::validate`, and (b) eventually a CI-level test asserting the
pipeline orchestrator routes Mapper output through Validator before
Generator. Stage 1 ships only (a); orchestrator integration is S6 / P-15
(eval-suite expansion).**

Acceptable because the Generator currently has no Mapper to receive
from — the deterministic happy path uses pre-curated `MappingPlan`s
that bypass the LLM. Article III's enforcement point is built; it
becomes load-bearing when S4 wires Mapper.

## Q6: Async or sync `validate()`?

**Decision:** **Async**, because `KnowledgeService::fetch_schema` is async
(it can hit network on cache miss). Tests use `#[tokio::test]`.

## Q7: Aggregate errors or fail-fast on first?

**Decision:** **Aggregate.** `validate()` walks every resource and
attribute, collects all errors, returns the full report.

Why: a Mapper run that fails 5 resources should surface all 5 to the
user in one pass — debugging "fix attribute, re-run, find next error"
loops is exactly the operator pain Article IV exists to prevent.

## Q8: Warnings — fail closed (treat as errors) or report?

**Decision:** **Report only.** `passed = errors.is_empty()`. The CLI
surfaces warnings to the user; they decide whether to proceed.

This matches the spec's "errors blocking; warnings surfaced but don't
block" line. Stage 5 may add a `--strict` flag that elevates warnings
to errors.
