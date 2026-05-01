---
name: eval-design
description: Loaded when designing a new golden migration or eval. Use when adding to libs/eval or terrashift-evals.
---

When designing a new eval:

1. **Source TF is realistic but minimal.** ~5-15 resources for Stage 1; up to 100+ for Stage 5 structural-fidelity tests.
2. **Expected target TF is hand-curated and reviewed by a teammate.** The golden output is the source of truth — if the eval ever drifts, it's the implementation that's wrong, not the golden.
3. **Document which constitution article(s) this eval validates.** Common patterns:
   - Article III — eval verifies Validator catches a hallucinated attribute.
   - Article VI — eval verifies version-pinning produces identical output across runs.
   - Article XII — eval has a documented per-tier token-cost ceiling.
4. **Document the expected token-cost ceiling.** Per Article XII rule 1, every eval has a per-tier ceiling. CI gates against rolling baseline (Article XII rule 4 — >30% increase blocks).
5. **Document the wall-clock time ceiling.** Per terrashift_plan.md Stage 1 exit criteria — p95 medium repo migration <60 seconds.
6. **Add to `terrashift-evals/README.md`** with full provenance:
   - Source: who authored it, what real-world repo it derives from, license
   - Coverage: which pipeline components it exercises
   - Constitution articles validated
   - Token cost baseline
   - Wall-clock baseline
7. **Cite Article III** in PR description.

Eval categories (per terrashift_plan.md §6.6):

- **Validity** — generated HCL is syntactically valid (`terraform validate`)
- **Plan cleanliness** — `terraform plan` shows no surprise drift
- **Attribute coverage** — every source attribute has a target equivalent (or explicit "not-yet-supported")
- **Cost delta accuracy** — Infracost diff matches expected within tolerance
- **Quality** — human-rated 1-5 via pairwise comparison (manual; not automated)

Avoid synthetic edge cases that don't reflect real customer pain. Prefer real-world repos (PratikMahajan AWS-to-Azure, Ben Foster GCP write-up, terraformer-generated examples).
