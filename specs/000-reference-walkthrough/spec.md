# Spec — P-00: Reference walkthrough and Terrashift mapping

**Status:** in progress
**Branch:** `000-reference-walkthrough`
**P-NN:** P-00 (terrashift_prompts.md lines 17-78)
**Constitution:** Article II (reference codebase discipline), XI (foundational mapping doc)

## Goal

Produce `TERRASHIFT_MAPPING.md` at the workspace root — the canonical document
that maps every Stakpak architectural seam to its Terrashift equivalent. This
becomes the decoder ring every subsequent P-NN prompt cites when implementing
a component.

## Non-goals

- No Rust code written this feature.
- No new agents, tools, or runtime configuration.
- This is pure synthesis from `refs/stakpak_arch.md` + `refs/stakpak/` source +
  `refs/claude-code/` source.

## Success criteria

`TERRASHIFT_MAPPING.md` contains six sections (A-F) per the P-00 prompt:

A. Table mapping the **11 seams** from `stakpak_arch.md` §39 to Terrashift
   crate locations and effort estimates.

B. Table mapping the **13 domain artifacts to replace** from §40 to Terrashift
   versions.

C. **6-phase mirror sequence** from §41 mapped to Terrashift stages.

D. Which **Article XIII anti-patterns** are most relevant to Terrashift's
   first three months and why.

E. **Anti-pattern → constitution article** cross-references.

F. **2-3 places where Claude Code's pattern is cleaner than Stakpak's** —
   adopt those in Rust idioms.

## Out of scope (Spec Kit clarifications addressed inline below)

The full P-00 prompt (`terrashift_prompts.md` lines 17-78) calls for spot-reading
Stakpak source for 3-5 patterns to verify the doc's description matches code.
For this initial pass we ship the mapping doc; verification is deferred to the
first time we hit a discrepancy in a downstream P-NN prompt. This is a
deliberate scope cut, not a skipped step — the discipline is "verify when you
USE the pattern, not preemptively."

## Inputs

- `refs/stakpak_arch.md` §1-13, §18-25, §26-30, §39-42 (the explicit P-00 reading list)
- `refs/stakpak/` source — for spot-checking
- `refs/claude-code/` source — for §F (Tool.ts, QueryEngine.ts, commands/, services/compact/)
- `terrashift_plan.md` — to anchor mapping in our domain
- `CONSTITUTION.md` — for §D and §E article references

## Output

Single file at workspace root: `TERRASHIFT_MAPPING.md`.
