<!--
PR template for Terrashift.

Per CLAUDE.md, every PR description MUST include the three sections below.
The code-reviewer agent and constitution-checker hook check for them.
Delete the comments and the "Summary / Test plan" section is optional.
-->

## Summary

<!-- 1-3 bullets. What changed and why. -->

## Test plan

<!-- Bulleted checklist of what you ran / what reviewer should run. -->
- [ ] `cargo fmt -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo test --package terrashift-eval --release` (if eval-touching)

## Constitution

<!--
Cite the article(s) this change is governed by, with a one-line rationale.
Article XIII rule numbers go on a separate line. Required.
-->
- Article N (rationale)
- Article XIII rule M (where applicable)

## stakpak_arch.md

<!--
Cite the section(s) you patterned after. Required when adding/refactoring
seams. "N/A — pure bug fix" is acceptable for narrow fixes.
-->
- section N (what was mirrored)

## Eval impact

<!--
Per Article XII rule 4. Required.
- For pure refactors with no LLM-path change: "0 (refactor)"
- For LLM-path changes: token cost delta vs baseline (run `cargo test -p terrashift-eval --release`)
-->
- Token cost delta vs baseline:
