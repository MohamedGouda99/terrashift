---
description: Run a stage-gate review per prompt P-16. Read-only review, no code changes.
---

Run the stage-gate review per prompt P-16 in `terrashift_prompts.md`.

Before producing the review, **confirm with the user which stage we are gating** (1, 2, 3, 4, 5, or 6). The criteria differ per stage.

Read the gate criteria from `terrashift_plan.md` section 17 ("Implementation stages"). For each criterion: **PASS / FAIL / PARTIAL** with evidence.

For Stage 1 (the most relevant gate today), check:

1. Demo passes for 15-resource sample end-to-end (run `cargo test -p terrashift-eval --release -- --include-ignored`).
2. Audit log signed (verify chain integrity per `libs/audit::verify_chain`).
3. Re-running migrate produces identical output (Article VI version-pinning + Article VIII reproducibility).
4. Token cost <$15 for sample migration (Article XII).
5. All 13 constitution articles cited in at least one PR (`gh pr list --state all --search "Article" --json title,body`).
6. Single-binary distribution works (binary doesn't depend on host's library state — Article XIII rule 7).

End with go/no-go recommendation. Findings that block must include a remediation ticket.
