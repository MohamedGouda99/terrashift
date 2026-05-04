# Terrashift documentation index

Everything written about Terrashift, organized by what you need it for.
Source code citations like `// Pattern: stakpak_arch.md section 8` resolve
against `docs/reference/stakpak_arch.md`.

## Where to start

| If you are... | Read first |
|---|---|
| **A new contributor (any role)** | [`onboarding/terrashift-mvp-onboarding.md`](onboarding/terrashift-mvp-onboarding.md) |
| **A new Claude Code user** | [`onboarding/CLAUDE-HANDOVER.md`](onboarding/CLAUDE-HANDOVER.md) |
| **About to open a PR** | [`governance/CONSTITUTION.md`](governance/CONSTITUTION.md) |
| **Setting up the workspace** | [`governance/pre-flight.md`](governance/pre-flight.md) |
| **Implementing a Stage-N component** | [`architecture/terrashift_plan.md`](architecture/terrashift_plan.md) + [`architecture/TERRASHIFT_MAPPING.md`](architecture/TERRASHIFT_MAPPING.md) |
| **Looking for a P-NN prompt** | [`development/terrashift_prompts.md`](development/terrashift_prompts.md) |

## Directory layout

```
docs/
├── README.md                        ← this file
│
├── onboarding/                      ← read these first when you join
│   ├── terrashift-mvp-onboarding.md  Comprehensive 1,257-line deep-dive
│   ├── CLAUDE-HANDOVER.md            Claude-Code-specific handover + history
│   └── terrashift_opus_setup.md      Per-machine Opus 4.7 setup notes
│
├── governance/                      ← the rules
│   ├── CONSTITUTION.md               13 articles. Cite in every PR.
│   ├── pre-flight.md                 Environment-specific decisions (paths, OS)
│   └── SESSION_PLAN.md               Multi-session path-to-production roadmap
│
├── architecture/                    ← what we're building
│   ├── terrashift_plan.md            High-level architecture document
│   ├── TERRASHIFT_MAPPING.md         Stakpak seam → Terrashift mapping (P-00)
│   ├── terrashift_diagrams.html      Architecture diagrams (rendered)
│   └── Terrashift_Plan.docx          Word version of the plan (ref-only)
│
├── development/                     ← how we build it
│   ├── terrashift_prompts.md         Implementation prompt library (P-00 → P-NN)
│   └── speckit_commands.txt          Spec Kit slash-command cheat sheet
│
└── reference/                       ← canonical external references
    └── stakpak_arch.md               ~2,840-line architectural analysis (PRIMARY)
```

## Conventions

- **Citations.** Source-code comments cite `stakpak_arch.md section N` (bare,
  no path) and `terrashift_plan.md §N`. These resolve via repo-wide search;
  don't refactor them to absolute paths because that breaks if files move
  again.
- **Constitution articles.** Cite as "Article N" or "Article XIII rule M" in
  PR descriptions. The PR template (`.github/PULL_REQUEST_TEMPLATE.md`) has
  the section.
- **Bare names vs. paths.** Documents that mention each other by bare name
  (e.g., `pre-flight.md`) keep working as long as the file exists *somewhere*
  in the repo. Documents that mention paths (e.g., `docs/governance/pre-flight.md`)
  pin the location and break on moves.

## What's NOT in here

- **`refs/`** is gitignored. Per-developer reference clones via
  `scripts/setup-refs.ps1`. Reason: license boundaries (Claude Code source is
  not redistributable) and repo-bloat avoidance (Stakpak source clone is 22 MB).
  The one piece that *is* tracked — `stakpak_arch.md` — lives at
  `docs/reference/stakpak_arch.md`.

- **`/specs/`** lives at the repo root, not under `docs/`. That's the Spec Kit
  convention; moving it would break `/speckit-*` slash commands.

- **`.specify/`, `.claude/`** ship their own infrastructure. See
  `onboarding/CLAUDE-HANDOVER.md` for what's in each.
