# Implementation Plan — P-00

## Reading order (from `refs/stakpak_arch.md`)

| Step | Section(s) | Why | Output to |
|---|---|---|---|
| 1 | §39 (the seams) | Source of §A table | TERRASHIFT_MAPPING.md §A |
| 2 | §40 (domain to replace) | Source of §B table | TERRASHIFT_MAPPING.md §B |
| 3 | §41 (mirror sequence) | Source of §C table | TERRASHIFT_MAPPING.md §C |
| 4 | §42 (anti-patterns) | Source of §D + §E | TERRASHIFT_MAPPING.md §D, §E |
| 5 | §8 (agent-core kernel) | Anchors §A row 2-5 (ToolExecutor, AgentHook, CompactionEngine, ContextReducer) | Cross-references in §A |
| 6 | §9 (shared+api) | Anchors §A row 6 (SessionStorage) | Cross-references in §A |
| 7 | §10 (stakai SDK) | Anchors §A row 1 (Provider trait) | Cross-references in §A |
| 8 | §16 (TUI) | Anchors §A rows 10-11 (mpsc channels, input mapping) | Cross-references in §A |

## Reading order (from `refs/claude-code/`)

| Step | File | Why | Output to |
|---|---|---|---|
| 1 | `Tool.ts` | Conceptual Tool shape vs Stakpak's | §F candidate #1 (Tool trait shape) |
| 2 | `commands/` (directory listing) | Per-file slash command pattern | §F candidate #2 (slash commands) |
| 3 | `services/compact/` | Three-mode compaction (manual / threshold / reactive) | §F candidate #3 (compaction modes) |

## Constitution articles to cite in TERRASHIFT_MAPPING.md

- Article I (architectural restraint — only 3 agents) — §C
- Article III (Validator on path) — §D
- Article V (credentials) — §D
- Article XII (token economy) — §C (which stage gates token-economy CI)
- Article XIII rules 1, 3, 4, 5, 6 — §D and §E

## Output structure

```
TERRASHIFT_MAPPING.md
├── Header (purpose, status, citation back to P-00)
├── A. The 11 seams from §39
│   └── Table (Stakpak seam | Terrashift equivalent | Crate/file | Effort)
├── B. The 13 things to replace from §40
│   └── Table (Stakpak artifact | Terrashift version | Notes)
├── C. The 6-phase mirror sequence from §41
│   └── Table (§41 phase | Effort | Maps to v5 Stage | Status)
├── D. Article XIII anti-patterns: top 5 for first 3 months
│   └── Numbered list (rule N, why now, where it hits in our P-NN sequence)
├── E. Anti-pattern → constitution article cross-references
│   └── Table (Article XIII rule | Parent article | Where enforced)
└── F. 2-3 Claude Code patterns cleaner than Stakpak
    └── Numbered list (pattern, where Claude Code does it, where Terrashift adopts it)
```

## Acceptance per Spec Kit checklist

- All 6 sections present (A-F)
- Every row in §A has a Terrashift crate path or "drop"
- Every row in §B has either "drop" or a Terrashift target
- §C has both Stakpak phase number AND v5 stage number
- §D has exactly 5 entries (per clarify Q3 decision)
- §E mappings match `CONSTITUTION.md` Article XIII table
- §F has 2-3 entries with explicit "Stakpak weakness → Claude Code fix" reasoning
- File compiles to valid markdown (no broken links, table syntax correct)
- Cited at top with `Pattern: stakpak_arch.md sections 39-42` (per Article II)
