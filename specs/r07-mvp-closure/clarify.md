# Clarify pass — r07-mvp-closure

**Mandate**: per `CLAUDE.md`, /speckit-clarify is non-skippable. This document
captures the ambiguities the spec surfaced + the resolution applied. Each is
an explicit decision so reviewers see the reasoning, not just the result.

## Q1 — Which schema version drives the "required attributes" prompt context?

**Ambiguity**: `KnowledgeService::fetch_schema(provider, version)` requires
a version. The Mapper today doesn't pick a target version explicitly — the
`MappingPlan` only carries `target_provider`.

**Options considered**:
- (a) Add a `--target-schema-version` CLI flag, propagate to Mapper.
- (b) Mapper queries `SchemaStore::list_versions(target_provider)` and uses
  the first (most recently cached) version.
- (c) Hard-code a version per Stage 1 (e.g., `azurerm@4.71.0`).

**Decision**: **(b)** — `list_versions(provider)[0]`. Reason: the user already
ran (or will run) `terrashift schema update --provider azurerm --version X`;
the Mapper consumes whatever's in the cache. Adding a CLI flag is a separate
UX concern (P-18 territory). Hard-coding violates Article VI.

**Edge**: if `list_versions` returns empty, Mapper falls back to the "no
required-attributes hint" path. Validator catches downstream gaps.

## Q2 — Failure mode when schema cache has no entry for the target provider

**Ambiguity**: graceful degradation vs hard fail.

**Decision**: **graceful degradation**. The Mapper prompt drops the
"required attributes" block and emits a `tracing::warn!` line. The
`VALID TARGET TYPES` block from `TemplateRegistry.stage1()` is independent
of the schema cache, so it always renders. Validator (Article III gate)
remains the load-bearing check downstream.

**Reason**: a hard fail here would block any migrate run on a binary that
hadn't run `schema update` yet. The bundled seed populates the cache on
first launch, but a user who deletes `~/.terrashift/schemas/` shouldn't get
a migrate-blocking error before they can re-populate.

## Q3 — Default-output naming when source is `.` or one level up

**Ambiguity**: `terrashift migrate --source . --to azurerm` cannot use
"parent's name + suffix" because "." has no meaningful name to derive from.

**Decision**: when `Path::file_name()` is `None` or `.` or `..`, fall back to
`./terrashift-output-<target>/` as the default. Otherwise use
`<canonical-source-name>-<target>/` next to the source.

**Test cases**:
- `--source ./aws-app` → `./aws-app-azurerm/`
- `--source ./aws-app/` (trailing slash) → `./aws-app-azurerm/` (Path::file_name strips the slash)
- `--source .` → `./terrashift-output-azurerm/`
- `--source /home/user/aws-app` → `/home/user/aws-app-azurerm/`
- `--source ../aws-app` → `../aws-app-azurerm/`

## Q4 — Suggestion algorithm when LLM emits unsupported target_type

**Ambiguity**: my spec mentioned "Levenshtein distance ≤ 3 closest match".
That needs either a dependency (`strsim`) or an inline impl.

**Decision**: **skip the suggestion algorithm for Stage 1**. The error message
just lists the supported set verbatim. Reasoning:
- The supported set is ~17 entries. The user (or Recovery agent in S10) can
  visually scan it.
- Adding `strsim` for one error message is dependency overhead.
- Recovery agent (S10) will do better than Levenshtein anyway — it'll
  re-prompt the LLM with the failure context.

`MapperError::UnsupportedTargetType` carries the supported set as a `Vec<String>`
so display can render it inline; no `suggestion` field.

## Q5 — Wrong-target-provider rejection

**Ambiguity**: if the LLM declares `target_provider = "azurerm"` but emits
a `target_type = "aws_vpc"`, what happens?

**Decision**: **reject with `MapperError::TargetProviderMismatch`**. The
existing supported-target-type validation (FR-4) catches this transitively
because `aws_vpc` isn't in the azurerm template set. So no separate variant
needed; the `UnsupportedTargetType` error already names the issue.

**Decision update**: drop FR-4's `suggestion` field per Q4; the existing
"supported set listed inline" wording handles this case as well.

## Q6 — `SYSTEM_PROMPT` cache invalidation

**Ambiguity**: Article XII rule 2 mandates cache stability; the Mapper cache
key includes the prompt content. Bumping the version string `Terrashift Mapper v1 → v2`
invalidates every cached mapping.

**Decision**: **accept the cache wipe**. This is the only correct behavior:
the new prompt produces structurally different output (now-required attribute
fields), so reusing v1 cached results would emit incomplete plans. Article XII
rule 2 explicitly accepts intentional bumps; the discipline is "don't move the
goalposts silently" — the version-string bump in source IS the goalpost move,
documented + reviewable.

## Q7 — Empty schema cache: what happens to `terrashift migrate`?

**Ambiguity**: a fresh install with no `terrashift schema update` ever run.
Today the Validator step uses the cache; with the new prompt, the Mapper
also uses the cache. Both layers degrade gracefully? Or does migrate fail?

**Decision**:
- **Mapper**: degrades per Q2 (no required-attrs hint, prompt still works).
- **Validator**: degrades per existing v0.1.0 behavior (KnowledgeService's
  `seed_from_bundle` populates the cache on first launch from the bundled
  seed; if the seed is missing, Validator's `fetch_schema` returns
  `Err(SchemaError::NotFound)` and the Validator surfaces an explicit
  "schema not cached" gap rather than a misleading "missing required attribute"
  one).

This is consistent with the post-PR-#6 model where the bundled seed +
`terrashift schema update` populate the runtime cache.

## Q8 — Tests: does Mapper need a real-LLM integration test, or can stub-only suffice?

**Ambiguity**: the existing `pratik_e2e_test.rs` is `#[ignore]`'d on
`HF_TOKEN`. New Mapper validation should be testable without an LLM round-trip.

**Decision**: **stub-only for unit tests**, real LLM for `#[ignore]`'d e2e.
- Unit tests: feed `StubClient` various malformed responses (empty target_type,
  unsupported target_type, complete required attrs, missing required attrs),
  assert each error variant fires correctly.
- E2E test: re-run `fixtures/e2e-aws-to-azure/` against a real LLM (manual,
  per-developer with `HF_TOKEN`); assert ≥4 of 5 resources emit successfully.

## Resolution summary

All eight ambiguities now have explicit decisions in code or in this document.
Implementation can proceed without further questions to the founder.
