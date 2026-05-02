# Tasks — P-00

Atomic dependency-ordered tasks. Each is 5-15 lines of work in TERRASHIFT_MAPPING.md.

## Tasks

- [x] T1: Read `refs/stakpak_arch.md` §39 (lines 2584-2601) — extract 11 seams
- [x] T2: Read `refs/stakpak_arch.md` §40 (lines 2602-2622) — extract 13 domain replacements
- [x] T3: Read `refs/stakpak_arch.md` §41 (lines 2623-2700) — extract 6 phases with effort estimates
- [x] T4: Read `refs/stakpak_arch.md` §42 (lines 2701-2715) — confirm 10 anti-patterns match `CONSTITUTION.md` Article XIII
- [ ] T5: Write `TERRASHIFT_MAPPING.md` header (purpose, status, citation)
- [ ] T6: Write §A — 11 seams table (Stakpak seam | Terrashift equivalent | Crate path | Effort)
- [ ] T7: Write §B — 13 replacements table (Stakpak artifact | Terrashift version | Notes)
- [ ] T8: Write §C — 6-phase sequence table (§41 phase | Effort | v5 stage | Status)
- [ ] T9: Write §D — top 5 anti-patterns for first 3 months (numbered list with "why now")
- [ ] T10: Write §E — anti-pattern → constitution article mapping table
- [ ] T11: Write §F — 2-3 Claude Code patterns to adopt (clear "Stakpak weakness → Claude Code fix" reasoning per clarify Q4)
- [ ] T12: Verify markdown renders cleanly (no broken tables, no broken links)
- [ ] T13: Commit `chore(p00): TERRASHIFT_MAPPING.md` per CONSTITUTION Article VII

## Dependencies

T1-T4 are independent (parallel reads). T5-T11 must come after T1-T4 but are
themselves parallel (independent file writes within one MD). T12-T13 are
sequential at the end.
