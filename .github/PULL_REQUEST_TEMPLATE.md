<!--
PR template for Terrashift.

Every PR must include the three required sections below. The CI pipeline
verifies the format.
-->

## Summary

<!-- 1-3 bullets. What changed and why. -->

## Test plan

<!-- Bulleted checklist of what you ran / what reviewer should run. -->
- [ ] `cargo fmt -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo test --package terrashift-eval --release` (if touching the LLM path)

## Conventions

<!--
Which project conventions does this change invoke? Examples:
- Bounded agents (only 3 named agents permitted; new agents need RFC)
- Article III Validator gate (no HCL emit before Validator approval)
- Loud failure (no silent stalls; all terminal states are explicit)
- No `unwrap` / `expect` in production code
- Cache-stable / monotonic context boundaries (Anthropic prompt cache)
- Audit on every tool execution (Ed25519 chain integrity)
Required.
-->
- Convention N (rationale)

## Architectural pattern

<!--
Cite the section / file from the architecture reference you patterned after.
Required when adding or refactoring a seam (trait, agent, hook, etc.).
"N/A — pure bug fix" is acceptable for narrow fixes.
-->
- pattern / section N (what was mirrored)

## Eval impact

<!--
Required.
- For pure refactors with no LLM-path change: "0 (refactor)"
- For LLM-path changes: token cost delta vs baseline
  (run `cargo test -p terrashift-eval --release`)
-->
- Token cost delta vs baseline:
