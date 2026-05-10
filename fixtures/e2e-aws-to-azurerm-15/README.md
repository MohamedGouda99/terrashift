# 15-Resource AWS → AzureRM E2E Fixture

Closes Stage 1 MVP gate criterion #1 ("Demo passes for 15-resource sample end-to-end" — `terrashift_plan.md` §17 + `SESSION_PLAN.md` §444).

## What's here

15 source AWS resources spread across 6 resource types:

| Type | Count | Source addresses |
|---|---|---|
| `aws_vpc` | 1 | `main` |
| `aws_subnet` | 3 | `public_a`, `private_a`, `db_a` |
| `aws_security_group` | 3 | `web`, `app`, `db` |
| `aws_instance` | 3 | `web_1`, `web_2`, `app_1` |
| `aws_s3_bucket` | 3 | `uploads`, `logs`, `backups` |
| `aws_iam_role` | 2 | `app_role`, `db_role` |

Total: **15 resources**.

## Expected migration outcome

Running `terrashift migrate --from aws --to azurerm` against this
fixture should emit **at least 12 of 15** target HCL resources. The
2 `aws_iam_role` resources are expected to skip (no `azurerm_role_*`
template registered in Stage 1) — that's by design, surfaces in the
"Skipped (gaps)" section, and meets the gate criterion's threshold.

## Why this shape

- **6 resource types** stresses the cross-type dispatch in
  Mapper + Generator (5-resource fixture only exercised 5 types one
  time each).
- **3 instances of 4 types** stresses iteration: same Mapper prompt
  fires on three peers; backup-first wrapper handles per-type files
  cleanly.
- **2 known-failing resources** (IAM roles) anchor the "≥12/15"
  threshold and verify the Skipped-gaps reporting actually surfaces
  the gap.
- **No modules / no count / no for_each** — Stage 1 scope per
  `terrashift_plan.md` §11 ("Stage 4 adds: dynamic blocks, complex
  modules, multi-region, count/for_each").

## What runs against this fixture

- `libs/engine/tests/e2e_15_resource_test.rs` — deterministic test
  using a hand-curated `MappingPlan` (no LLM dependency in CI). Runs
  Validator → Generator and asserts ≥12/15 emit.

## Article citations

This fixture is referenced by tests citing Article III (Validator on
the path), Article VI (deterministic emission), and Article VIII
(closes Stage 1 gate criterion #1).
