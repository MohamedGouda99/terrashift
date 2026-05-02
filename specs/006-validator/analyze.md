# Analysis — P-06 (post-implementation)

## Decisions taken at implementation time

### A. Article-citation prefixes in error `Display` impls

Spec just says "validation errors are loud per Article IV." Implementation
embeds explicit "Article III violation:" / "Article IV violation:" prefixes
in `ValidationError`'s `Display`. This deviates from Scanner / Generator
precedent — those cite articles only in module doc comments, not runtime
strings.

Why deviate: Validator is the single Article III enforcement point. Ops
triage on `terraform plan` failure reads the Validator's stderr first;
if that string already says which article is violated, the operator can
look up the article and fix the upstream Mapper bug without reading
source code. The verbosity earns its keep.

### B. File-level `#![allow(clippy::unwrap_used, clippy::expect_used)]` in test

Discovered during `cargo clippy` run: workspace `clippy.toml`'s
`allow-unwrap-in-tests = true` only relaxes the lint inside `#[test]`
functions and `#[cfg(test)]` modules — NOT plain `async fn` helpers in
integration test files (which are compiled as separate test crates).

`make_knowledge_with_aws_schema()` is async (because `LocalSchemaStore::in_memory`
is async) and has 2-3 `.unwrap()` calls. File-level allow keeps test
ergonomics clean; the alternative (Result-propagating helpers) inflates
ceremony across 7 tests for no signal.

Code-reviewer NICE-TO-HAVE confirmed this is the right tradeoff.

### C. Removed `required_attribute_count` helper

Spec'd in `plan.md` as "exposed for tests + future diagnostic surfaces."
Implementation initially included it with `#[allow(dead_code)]`. Code-
reviewer flagged YAGNI. Removed — easier to re-add 4 lines when
something actually needs it than carry dead code through reviews.

### D. Async `validate()`

Same boundary as `KnowledgeService::fetch_schema`. Cache-miss path can
hit the network, so the API has to support async naturally.
Code-reviewer agreed.

## Risks resolved

| Risk (from plan.md) | Outcome |
|---|---|
| Test fixture KnowledgeService too heavy to set up per-test | `make_knowledge_with_aws_schema` is ~12 lines, used by all 7 tests |
| `LocalSchemaStore::in_memory` SQLite races concurrent tests | Each test calls `in_memory()` for a fresh isolated pool |
| Validator-Knowledge async lifecycle | `Arc<KnowledgeService>` worked first try; no surprises |

## What's NOT here (deferred)

- **Type-system gate** (clarify Q4) — `AttributeSchema.attribute_type`
  is an HCL type expression string. Parsing it requires substantial
  effort (~500 LOC). Stage 5 / P-27. **Code-reviewer flagged this as a
  MAJOR-but-deferrable: a Mapper that emits `cidr_block: 42` (number
  vs string) would pass current tests because Validator only checks
  attribute name presence.** Worst case caught at Executor (P-09)
  `terraform plan` time. Acceptable Stage 1 trade-off.
- **Cross-resource constraint validation** — e.g., a subnet's `vpc_id`
  reference must resolve. Stage 5.
- **Module-level validation** — Stage 5.
- **Provider-version compatibility windows** — Stage 5.

## Stakpak fidelity

No Stakpak counterpart for migration-domain Validator. Closest analog:
`stakpak_arch.md §27` (single-redaction-enforcement-point). Mirrored
in spirit only — one place where the AI-safety invariant gets enforced,
bypassing it is a CI failure.

## Spec criteria coverage

All 7 numbered criteria from spec.md have a test:

| Criterion | Test |
|---|---|
| #1 valid plan passes | `valid_plan_passes` |
| #2 hallucinated attr fails (Article III) | `hallucinated_attribute_fails_with_article_iii_message` |
| #3 missing required fails (Article IV) | `missing_required_attribute_fails` |
| #4 unknown resource type fails | `unknown_resource_type_fails` |
| #5 deprecated attr warns | `deprecated_attribute_warns_not_blocks` + `setting_computed_attribute_warns` |
| #6 multiple errors aggregate | `multiple_errors_aggregate_not_fail_fast` |
| #7 clippy clean | enforced by `cargo clippy --workspace --all-targets -- -D warnings` |
