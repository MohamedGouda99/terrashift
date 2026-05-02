# Spec — P-06: Validator (Article III enforcement gate)

**Stage:** 1 | **P-NN:** P-06 | **Tier:** All
**TERRASHIFT_MAPPING.md:** §A row 2 (ToolExecutor — ValidatorTool impl Tool),
§B row 1 (migration tools — `validate`).
**Constitution:** Article III (single AI safety enforcement point;
bypassing this is a CI failure), Article IV (validation errors are loud),
Article XIII rule 3 (no unwrap/expect/string-slice in production).
**Source pattern:** Terrashift-specific. No Stakpak counterpart — this is
a migration-domain concern. The closest analog is Stakpak's privacy/redaction
gate (`stakpak_arch.md §27`) which is also a single enforcement point.

## Goal

Take a `MappingPlan` (Mapper output, eventually) and verify every target
attribute against the live target-provider schema fetched via
`KnowledgeService`. Hallucinated attributes fail loudly. Missing required
attributes fail loudly. Deprecated attributes warn but don't block.

This is the **single most important Article III enforcement point**. If
the Mapper's LLM hallucinated `aws_vpc.cidr_blocks` (plural — wrong),
Validator catches it before HCL emit and the migration aborts with a
clear error.

## Stage 1 scope

- `Validator` struct + `validate(plan, target_version) -> ValidationReport` API
- `ValidationReport { passed, errors: Vec<ValidationError>, warnings: Vec<ValidationWarning> }`
- `ValidationError` variants:
  - `UnknownResourceType { addr, target_type }` — `target_type` not in `ProviderSchema.resources`
  - `UnknownAttribute { addr, attr, target_type }` — `attr` not in `ResourceSchema.attributes`
  - `MissingRequiredAttribute { addr, attr, target_type }` — schema marks attr `required: true` but plan omits it
- `ValidationWarning` variants:
  - `DeprecatedAttribute { addr, attr, since }` — attr `deprecated: Some(...)`
  - `SetComputedAttribute { addr, attr }` — schema `computed: true` && not `optional` (read-only attrs shouldn't be set)
- Pure deterministic — NO LLM. Single async call to `KnowledgeService::fetch_schema`.
- `Validator` constructor takes `Arc<KnowledgeService>` — same composition pattern as Generator's `Arc<dyn AuditStore>` deferral (caller-driven).

## Stage 1 out of scope

- **Type checking** — `AttributeSchema.attribute_type` is an HCL type
  expression string ("string", "list(string)", "object({...})"). Parsing
  HCL types is non-trivial. Stage 1 records the type as a `String` and
  defers comparison; Stage 5 (P-27) adds a real type-system gate.
- **Cross-resource constraint validation** (e.g., "subnet.vpc_id must
  reference an `aws_vpc` resource that exists in the same plan") — Stage 5.
- **Module-level validation** (modules' input/output contract checking) — Stage 5.
- **Provider-version compatibility windows** — Stage 5.

## Success criteria

`cargo test -p terrashift-engine --test validator_test` covers:

1. **Valid mapping passes** — well-formed `MappingPlan` against a
   matching `ProviderSchema` returns `passed: true`, empty errors.
2. **Hallucinated attribute fails (Article III)** — attribute name not
   in schema → `ValidationError::UnknownAttribute` with `addr` + `attr`
   named in the message.
3. **Missing required attribute fails (Article IV)** — schema marks
   `required: true`, plan omits → `ValidationError::MissingRequiredAttribute`.
4. **Unknown resource type fails** — `target_type` not in schema →
   `ValidationError::UnknownResourceType`.
5. **Deprecated attribute warns (not blocks)** — `passed` stays `true`,
   `warnings` contains `DeprecatedAttribute`.
6. **Multiple errors aggregate** — bad plan with 3 issues produces 3
   `ValidationError` entries, not stops at first.
7. **Article XIII rule 3** — no `unwrap`/`expect`/string-slice in
   production paths.

All 4 build gates green: fmt + clippy + check + test.
