# Acceptance Checklist — P-00

## Output deliverable

- [x] `TERRASHIFT_MAPPING.md` exists at workspace root
- [x] All 6 sections present (A, B, C, D, E, F)
- [x] §A: 11 seams listed with Terrashift crate path + effort estimate
- [x] §B: 13+ domain artifacts listed with Terrashift version or "drop"
- [x] §C: 6 phases listed with Stakpak phase number AND v5 stage AND status
- [x] §D: exactly 5 anti-patterns called out with "why now" reasoning
- [x] §E: all 10 Article XIII rules mapped to parent article + enforcement file
- [x] §F: 2-3 Claude Code patterns with explicit "Stakpak weakness → Claude Code fix"

## Citations

- [x] Header cites `Pattern: stakpak_arch.md sections 39-42`
- [x] §A column "Crate / file location" populated for every row
- [x] §C entries cite both `stakpak_arch.md §41` phase AND v5 stage
- [x] §D each entry references the P-NN where it'll first hit
- [x] §E enforcement column cites file path or mechanism
- [x] §F each entry cites `refs/claude-code/<path>` for the reference shape

## Constitution

- [x] Article II (reference codebase discipline) cited at top
- [x] Article XI (this is the foundational mapping doc) cited in spec.md
- [x] No Article I (no new agents added)
- [x] No Article III bypass (Validator path discussion appears in §D)
- [x] Article V (credentials) referenced in §D rule 5/6 entries
- [x] Article XII (token economy) appears in §C and telemetry row of §B

## Spec Kit hygiene

- [x] `spec.md` written first
- [x] `clarify.md` answers 5 questions on user's behalf with rationale
- [x] `plan.md` lists reading order + acceptance criteria
- [x] `tasks.md` enumerates 13 atomic tasks
- [x] `analyze.md` cross-checks spec ↔ plan ↔ tasks ↔ output, finds no drift
- [x] This `checklist.md` confirms acceptance

## Quality gates

- [x] No dangling links in `TERRASHIFT_MAPPING.md`
- [x] Markdown tables render cleanly (no broken pipe alignment)
- [x] All cited `stakpak_arch.md` line numbers verified to exist
- [x] All cited `refs/claude-code/` paths verified to exist (`Tool.ts`, `commands/`, `services/compact/`)
- [x] No code committed (P-00 is documentation only — Article I.4 doesn't apply)

## Ready for commit

**Verdict:** ✅ All gates pass. Commit per Article VII (Conventional Commits).
