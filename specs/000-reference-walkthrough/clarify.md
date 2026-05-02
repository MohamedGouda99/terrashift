# Clarifications — P-00

User authorized "answer Spec Kit questions on my behalf using constitution + plan
as the decision tree." All decisions below applied that rule.

## Q1: How verbose should the §A and §B tables be?

**Options:** (a) one-line per row, ~30 rows total; (b) one-paragraph per row with
file:line citations, ~150 rows total.

**Decision:** **Option (a)**, one-line per row. Rationale: the P-00 spec calls
this an "onboarding artifact" — meant to be skimmed by reviewers and quickly
pointed-at from PR descriptions. Verbose explanations belong in the per-prompt
specs (P-02, P-04, etc.), not in the index doc. This matches Stakpak's own
`stakpak_arch.md` §39 table which is one-line-per-seam.

## Q2: Should §C list the v5 stage gates (Stage 1, Stage 2, …) or the §41 phases
(Phase 1-6)?

**Options:** (a) §41 phases verbatim with effort estimates; (b) translate to
v5 stages; (c) both — show §41 phase → v5 stage mapping.

**Decision:** **Option (c)**. The §41 phases ARE the canonical mirror sequence
(constitution Article II says cite Stakpak's structure). The v5 stages are how
Terrashift's team measures progress. Showing both lets a reader follow either
path. Cost: 2 extra columns. Worth it.

## Q3: For §D (anti-pattern relevance), how many to call out?

**Options:** (a) all 10 with relevance notes; (b) top 5 most likely to hit in
first 3 months.

**Decision:** **Option (b)**. The full Article XIII table is already in
`CONSTITUTION.md`. §D's job is to flag which ones to watch for during initial
implementation — that's a focused list, not a re-rendering. We surface 5 with
"why now" reasoning; reader knows to consult the full table for the rest.

## Q4: For §F (Claude Code patterns to adopt), how strict on "cleaner"?

**Options:** (a) only patterns where Claude Code is unambiguously better; (b)
include patterns where it's a stylistic preference.

**Decision:** **Option (a)**. We're constrained to 2-3 picks per the prompt.
The bar is "Stakpak's approach has a known weakness AND Claude Code's approach
solves it" — not "Claude Code does X differently." This keeps §F honest.

## Q5: Should we also list anti-patterns from `refs/claude-code/` that we should
NOT adopt?

**Options:** (a) yes, add a §G; (b) no, scope is explicit (sections A-F).

**Decision:** **Option (b)** — stay in scope. Note any Claude-Code-specific
quirks we don't adopt in the relevant downstream P-NN prompt (e.g., when P-14
adopts the per-file slash command pattern, that's where to call out which TS
quirks we're skipping).

---

All clarifications resolved. Proceeding to plan + tasks + implement.
