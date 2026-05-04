# Claude Code Handover — Picking up Terrashift with your own account

**Audience:** the next Claude Code (or other LLM-IDE) user joining this
project. You're inheriting an MVP-ready codebase. This doc tells you how
to set up your own Claude session, what artefacts already live in the
repo to support you, and the chronological development history so you
can pick up where the previous Claude session left off.

**Last updated:** 2026-05-04 (commit `a38eaf8` — MVP-ready milestone)

---

## Table of contents

1. [What you're inheriting](#1-what-youre-inheriting)
2. [Setting up your own Claude account on this repo](#2-setting-up-your-own-claude-account-on-this-repo)
3. [The `.claude/` workspace explained](#3-the-claude-workspace-explained)
4. [The `.specify/` workspace explained](#4-the-specify-workspace-explained)
5. [Reading order for new contributors](#5-reading-order-for-new-contributors)
6. [Development history — chronological commit-by-commit](#6-development-history--chronological-commit-by-commit)
7. [Where to start when you open Claude](#7-where-to-start-when-you-open-claude)
8. [How prior Claude sessions worked (operating patterns)](#8-how-prior-claude-sessions-worked-operating-patterns)
9. [Prompts library](#9-prompts-library)
10. [Active backlog (what's next)](#10-active-backlog-whats-next)

---

## 1. What you're inheriting

**Project status:** Stage 1 (deterministic core) + interactive TUI ✅
shipped. **278 tests pass, 0 failures.** Stage 2 (Recovery, Cost
Optimizer agents, real Infracost wiring) is next on the backlog —
structural code exists, runtime wiring does not.

**The codebase:**
- 16 workspace crates, one binary (`terrashift`)
- ~30k LOC of Rust + 1.2k-line onboarding doc + 130-line constitution
- Full Spec Kit feature history under `/specs/` (17 features specified)
- 17 Generator templates registered, 173 bundled provider schemas

**The collaborator workflow that produced it:**
- Spec Kit's 7-step inner loop (`/speckit-specify` → `/speckit-clarify`
  → `/speckit-plan` → `/speckit-tasks` → `/speckit-implement` →
  `/speckit-analyze` → `/speckit-checklist` → `/speckit-git-commit`)
- Constitution-driven PR descriptions citing Article N rule M
- Stakpak (`refs/stakpak/`) as the primary architectural reference,
  cited in code comments via `// Pattern: stakpak_arch.md section N`

**What's already wired for you in `.claude/`:**
- 5 specialised agents (code-reviewer, constitution-checker,
  eval-runner, reference-explorer, security-auditor)
- 5 slash commands (`/article`, `/plan`, `/refchk`, `/stage-gate`,
  `/token-audit`)
- 3 skills (eval-design, rust-tool-impl, stakpak-pattern)
- Pre-commit + pre-push hooks enforcing constitutional discipline
- MCP server config for read-only access to `refs/stakpak/` and
  `refs/claude-code/`

---

## 2. Setting up your own Claude account on this repo

### 2.1 Prerequisites

- Windows 11 or macOS / Linux (paths in `.claude/mcp.json` assume
  Windows; you'll retarget — see step 4)
- Rust 1.94+ (`rustup default 1.94.1`)
- Node 18+ (for `@modelcontextprotocol/server-filesystem`)
- Git
- Claude Code CLI installed and authenticated to your Anthropic account
- Optional: `terraform` 1.0+ on PATH (only needed for
  `terrashift schemas sync`)
- Optional: `HF_API_KEY` env var (for the real-LLM e2e tests)

### 2.2 Clone and bootstrap

```powershell
# Clone
git clone https://github.com/MohamedGouda99/terrashift.git
cd terrashift

# Build (~5 min cold)
cargo build --release

# Run the test suite (~30s, 278 tests)
cargo test --workspace --no-fail-fast

# Set up reference codebases (one-time, large download)
.\scripts\setup-refs.ps1     # Windows
# or `bash scripts/setup-refs.sh` if on macOS/Linux (port if needed)
```

### 2.3 Retarget the MCP filesystem paths

`.claude/mcp.json` ships with hardcoded Windows paths from the original
author. Edit them to match your machine:

```json
{
  "mcpServers": {
    "filesystem-readonly-refs": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "<YOUR-PATH>\\terrashift\\refs\\stakpak",
        "<YOUR-PATH>\\terrashift\\refs\\claude-code",
        "<YOUR-PATH>\\terrashift\\refs"
      ]
    }
  }
}
```

Replace `C:\\Users\\goda\\Desktop\\terrashift\\` with wherever you cloned
the repo. After editing, restart Claude Code so the new MCP config takes
effect.

### 2.4 Open Claude Code in the repo root

```powershell
cd terrashift
claude
```

When Claude starts, the project-level `CLAUDE.md` is auto-loaded.
You'll see Claude reference articles, MCP servers, agents, commands,
and skills automatically. Confirm by asking it: "list available agents
and skills" — it should mention `code-reviewer`, `eval-runner`,
`stakpak-pattern`, etc.

### 2.5 Local Claude state (NOT in the repo)

The following are per-user and live in `~/.claude/projects/<your-path>/`:

- `<session-id>.jsonl` — conversation transcripts (one per session)
- `memory/` — auto-memory built up across sessions (user profile,
  feedback patterns, project-specific facts the Claude sessions
  learn)

These are intentionally **not** in the repo. Each contributor accrues
their own. If you want to seed your memory with notes about how this
project works, look at section 8 (operating patterns) and copy the
useful parts into `~/.claude/projects/<your-path>/memory/MEMORY.md`.

---

## 3. The `.claude/` workspace explained

```
.claude/
├── agents/              # Specialised agents Claude can spawn
│   ├── code-reviewer.md         # Reviews PR diff against constitution
│   ├── constitution-checker.md  # Pre-commit lightweight check
│   ├── eval-runner.md           # Runs the golden-migrations eval suite
│   ├── reference-explorer.md    # Reads patterns from stakpak_arch.md
│   └── security-auditor.md      # Audits for credential / secret leakage
│
├── commands/            # Slash commands available in Claude
│   ├── article.md       # /article N — looks up constitution article N
│   ├── plan.md          # /plan — invokes Plan mode with rich context
│   ├── refchk.md        # /refchk — verify reference codebase patterns
│   ├── stage-gate.md    # /stage-gate — checks stage exit criteria
│   └── token-audit.md   # /token-audit — runs Article XII rule 4 check
│
├── skills/              # Reusable knowledge bundles
│   ├── eval-design.md           # How to design golden-migration evals
│   ├── rust-tool-impl.md        # Tool trait impl pattern
│   └── stakpak-pattern.md       # Citation discipline + lookup recipes
│
├── hooks/               # Git/Claude lifecycle hooks
│   ├── pre-commit.sh    # Lightweight constitutional check on staged diff
│   └── pre-push.sh      # Full clippy + test before pushing
│
├── mcp.json             # MCP server config (filesystem-readonly-refs, etc.)
└── settings.json        # Permission policy + plugin enable list
```

**`settings.json`** declares which Bash commands Claude can run without
asking (`cargo *:*`, `git status:*`, etc.) and which require approval
(`Edit`, `Write`, `git commit:*`, `git push:*`). It also disables the
50+ plugin marketplace defaults that don't apply to this project (no
Stripe, no Slack, no Notion, etc.) — keeps the tool surface focused.

**`settings.local.json`** is per-user and **not** committed. It's your
session-local Claude state.

---

## 4. The `.specify/` workspace explained

Spec Kit drives the inner loop. The directory structure:

```
.specify/
├── extensions.yml         # Top-level Spec Kit config
├── extensions/git/        # Git-aware Spec Kit extensions
├── integrations/          # Tool integrations (Claude Code, Codex, ...)
├── memory/                # Spec Kit's own memory (constitution + history)
├── scripts/               # Helper scripts (e.g., feature creation)
├── templates/             # Spec/plan/tasks/checklist boilerplate
└── workflows/             # Workflow definitions (the 7-step loop)
```

When you run `/speckit-specify add-foo-bar`, Spec Kit:
1. Creates `specs/0NN-add-foo-bar/spec.md` from `templates/spec-template.md`
2. Creates a feature branch `feat/0NN-add-foo-bar`
3. Returns control to you to fill in the spec

The 17 specs already in `/specs/` are the project's feature history —
read any of them to see what a "good" Terrashift spec looks like.

---

## 5. Reading order for new contributors

In strict order (don't skip):

1. **`CONSTITUTION.md`** — 130 lines, 13 articles. Foundation.
2. **`docs/onboarding/terrashift-mvp-onboarding.md`** — the comprehensive
   onboarding doc (1,257 lines, you're inheriting this).
3. **`terrashift_plan.md`** sections 1–9 — architecture + knowledge
   layer.
4. **`TERRASHIFT_MAPPING.md`** sections A and D — Stakpak seam mapping
   + top anti-patterns.
5. **`pre-flight.md`** — env-specific decisions (paths, OS).
6. **`CLAUDE.md`** — operating instructions for any Claude session.
7. **A real spec** — pick `specs/004-scanner/` and read `spec.md`,
   `clarify.md`, `plan.md`, `tasks.md` to feel the rhythm.

---

## 6. Development history — chronological commit-by-commit

45 commits from bootstrap to MVP. Each line tells you what landed and,
where useful, the spec dir or article it touched.

### D0 — Bootstrap (commits 1–5)

| # | Commit | What landed |
|---|---|---|
| 1 | `1db64bf` | Workspace per v5 — `Cargo.toml` with 16 members, base lints, base deps |
| 2 | `631ecf4` | CI workflow — fmt + clippy + test on every push |
| 3 | `cd2a463` | Move reference codebases inside the workspace at `refs/` (per pre-flight Decision 2) |
| 4 | `c301678` | Clone Stakpak fresh into `refs/stakpak/` instead of junctioning the local working copy (pre-flight Decision 2 rationale) |
| 5 | `0b0fde3` | D0 verified — `cargo check`, `cargo fmt`, `cargo clippy` all clean |

### Stage 1 — Deterministic core (commits 6–28)

| # | Commit | What landed |
|---|---|---|
| 6 | `439998f` | **P-00** `TERRASHIFT_MAPPING.md` — the Stakpak seam mapping (D1 deliverable) |
| 7 | `6d05e20` | **P-02** `ToolExecutor` + `AgentHook` + `ToolRegistry` in `libs/agent-core` |
| 8 | `f61dc31` | **P-04** Scanner — deterministic HCL parser via `hcl-rs` |
| 9 | `900094d` | `SESSION_PLAN.md` — multi-session path-to-production roadmap |
| 10 | `2b48216` | **P-07** RAG-ready Knowledge layer — schemas, vectors, embeddings, fetcher |
| 11 | `4545065` | **P-11** Audit log — Ed25519 signed, hash-chained, append-only |
| 12 | `48cf7f6` | Plan: mark S2 ✅, S3 🟡 partial; insert S3b for P-08 + P-12 |
| 13 | `07d26b3` | **P-08** Generator — deterministic HCL emit + reversible backup |
| 14 | `311b12c` | **P-12** Eval framework — golden-file harness + 3 fixtures |
| 15 | `3525903` | **P-06** Validator — Article III enforcement gate (non-bypassable) |
| 16 | `a01797c` | **S7** +2 Azure goldens; relax `build_resource_block` to accept References for required slots |
| 17 | `920b7a4` | **P-13** release pipeline + cliff.toml; activate eval CI gate |
| 18 | `b3a809c` | **P-14** TUI slash commands — per-file pattern from Claude Code |
| 19 | `e648dea` | **P-03** LLM client + 5-layer BYOK resolver (Groq integration test gated on `GROQ_API_KEY`) |
| 20 | `a1c76a1` | **P-05** Mapper — single-LLM-call structured output (S4b structural) |
| 21 | `44a37ce` | **S7** close — eval suite at 10/10 hand-curated goldens |
| 22 | `8509cc7` | **P-10** Credential broker — Article V cornerstone (S5 structural; STS/ADC/managed-identity gated on cloud creds) |
| 23 | `88fbc38` | **R06** Article XII rule 4 active CI regression gate (Stage 1 baseline + Lenient/Strict mode) |
| 24 | `d30862d` | **R05** Mapper-level RealClient `#[ignore]`'d integration test (S4-close swap-pattern proof) |
| 25 | `d951fb7` | **R02** 15-resource GCP→AWS full-stack fixture (Stage 1 P-16 #1 demo) |
| 26 | `556aa2f` | **P-09a** shell-tool-approvals — Article XIII rule 6 enforcement primitive |
| 27 | `e3f6e6d` | **P-09b** Executor structural — orchestration shell consuming P-09a's gate |
| 28 | `b1e3449` | Refactor — align `agent-core` with `stakpak_arch.md` §39 rows 4+5 (`ContextReducer` + `CompactionEngine` seams) |

### Stage 2 structural (commits 29–37)

| # | Commit | What landed |
|---|---|---|
| 29 | `5b0d264` | Refactor — align `libs/ai` with `stakpak_arch.md` §39 row 1 (`providers/{name}/` shape) |
| 30 | `950c886` | GitHub repo hygiene — templates, CODEOWNERS, dependabot, security |
| 31 | `8de2bc2` | **S9** Agent loop kernel — `run_agent` + `ApprovalStateMachine` + retry (Stage 2 narrowed) |
| 32 | `52d3af6` | **S10** Recovery agent — bounded ReAct loop + fix tools (Stage 2 structural) |
| 33 | `e58a843` | **S11** Cost Optimizer agent + `StubCostService` (Stage 2 structural) |
| 34 | `178132e` | Polished README + logo; **untrack internal governance docs** *(reversed in commit 46)* |
| 35 | `d971f73` | **S12+S13** WIF/OIDC token provider + notification system w/ Slack adapter |
| 36 | `7a6d92e` | **S14** eval suite expansion + 2 Stage-2 templates (`aws_iam_role`, `google_storage_bucket`) |
| 37 | `b5d46dd` | Naming — remove Stakpak brand references from source comments |

### S4-close + S17a + interactive UX (commits 38–45)

| # | Commit | What landed |
|---|---|---|
| 38 | `e0be47e` | **S10b** `JsonAgentLlmClient` adapter + profile example + smoke binary |
| 39 | `ad07fad` | **S4-close-prep** real LLM round-trip via HF Inference Providers + `pratik_e2e` test |
| 40 | `aa64f08` | **S17a** CloudForge JSON seed bootstrap + 5 azurerm Generator templates |
| 41 | `6eb98e6` | Knowledge — `provider/category/resource.json` seed layout + bulk converter (170 resources) |
| 42 | `a26d806` | **S17a** `TerraformCliSchemaFetcher` + `first_launch_sync` (real registry pull on cold start) |
| 43 | `30426c5` | CLI — real subcommands: `migrate`, `schemas {sync,list,seed}`, `scan` |
| 44 | `34d8278` | TUI — interactive ratatui session: `terrashift` no-args opens a session |
| 45 | `a38eaf8` | **MVP-ready** test: deeper coverage — CLI smoke, every-template, seed integrity, TUI (+47 tests, 231 → 278) |

---

## 7. Where to start when you open Claude

After section 2 setup is done, paste this into your first Claude
session to orient it:

> You are joining Terrashift mid-flight. Read these files in order:
> CLAUDE.md, CONSTITUTION.md, docs/onboarding/terrashift-mvp-onboarding.md,
> docs/onboarding/CLAUDE-HANDOVER.md. Then run `cargo test --workspace
> --no-fail-fast` and confirm 278 tests pass. After that, summarise the
> MVP scorecard and propose what we should pick up next from
> docs/onboarding/CLAUDE-HANDOVER.md section 10.

That gets Claude up to speed in ~3 minutes of reading. From there it
can suggest next-step PRs informed by the constitution and the backlog.

---

## 8. How prior Claude sessions worked (operating patterns)

The previous Claude sessions (Opus 4.7) followed a few patterns worth
inheriting:

### 8.1 Citation discipline

Every implementation cited the `stakpak_arch.md` section that informed
it. PR descriptions had three sections: **Constitution** (which articles
were touched), **stakpak_arch.md** (which sections were mirrored),
**Eval impact** (token cost delta vs baseline).

Code comments at the top of any pattern-borrowed file include:

```rust
//! Pattern: stakpak_arch.md section 8 (agent loop kernel).
//! Constitution: Article I (architectural restraint),
//!               Article XIII rule 1 (reducer always on the path).
```

### 8.2 Stage-gate awareness

When user said "do what recommended then go with the rest of other
sessions, plans and stages don't forget to follow all md files here
and use everything in .claude/ and in refs/. once mvp is ready let me
know even if i tell you proceed again remind me first that mvp is
ready for testing before proceedure" — the operating pattern was:

- Auto-progress through stages without asking
- BUT stop at MVP-ready boundary and remind the user
- Even if the user later says "continue", remind them MVP is ready
  before progressing

The previous session reached MVP-ready at commit `a38eaf8`. Treat
the next "continue" as needing user confirmation that they want
Stage 2 work to start.

### 8.3 Spec Kit non-skip

`/speckit-clarify` is non-skippable. Most ambiguities surface there;
resolving them before code is 10× cheaper than after. The 17 specs in
`/specs/` all have `clarify.md` files because of this discipline.

### 8.4 No silent failures (Article IV)

When previous sessions hit an unfamiliar state (e.g., a clippy lint
in test code, a parse error in a seed JSON), they surfaced it loudly
rather than papering over it. Article XIII rule 3 (no `unwrap()` /
`expect()` / `&s[..n]` outside tests) is workspace-deny in clippy.

### 8.5 Honest MVP testing

The session immediately before MVP-ready (commit `a38eaf8`) added 47
tests across 6 layers to convert "I think this works" to "I have
evidence this works." Subprocess CLI tests, every-template Generator
tests, seed JSON integrity tests, ratatui `TestBackend` tests. Read
the commit message for the full per-layer breakdown.

---

## 9. Prompts library

`terrashift_prompts.md` contains every implementation prompt P-00
through P-NN that was used to generate the codebase. Each prompt:

- Names a `stakpak_arch.md` section to read first
- Names a Stakpak source file to compare against
- Names the constitution article(s) the implementation must respect
- Names the test contract that proves the implementation is correct

These prompts are reusable when you (or a future Claude session) want
to add a new component using the same pattern. E.g., when you wire
Recovery into the runtime, prompt P-NN-Recovery-Wiring follows the
same shape as the originals.

---

## 10. Active backlog (what's next)

Sized by "post-MVP work that pays off most per LOC":

### Must-do for Stage 2 close

| Item | LOC est. | Spec dir | Notes |
|---|---|---|---|
| 1. Wire `JsonAgentLlmClient` into Recovery + Cost Optimizer integration tests | ~150 | `specs/016-recovery-agent/`, `specs/017-cost-optimizer/` | Uses existing structural code |
| 2. Real `InfracostCostService` adapter | ~250 + 6 tests | `specs/017-cost-optimizer/` | `INFRACOST_API_KEY` available |
| 3. FastEmbed semantic embedding swap | ~150 + new dep | new spec needed | Replaces `StubEmbeddingService`, big quality jump for Mapper retrieval |

### Should-do (improves MVP, not blocking Stage 2)

| Item | LOC est. | Notes |
|---|---|---|
| 4. In-TUI agent runtime (`/migrate` runs the pipeline interactively) | ~600–1000 | S6 polish; today the slash command points users at the shell |
| 5. Mapping examples corpus (50–100 hand-curated AWS↔Azure↔GCP equivalences) | ~500 + curation | Few-shot examples in Mapper prompt |
| 6. Eval harness for Mapper retrieval quality (recall@5 metric) | ~300 | Promotes retrieval quality to a CI signal |

### Nice-to-do (improves robustness)

| Item | LOC est. | Notes |
|---|---|---|
| 7. Branch protection on the GitHub repo | config | needs GitHub Pro upgrade or making repo public |
| 8. Hard-negative mining for Validator (semantic-mismatch rejection) | ~400 | needs production audit log data — long lead time |
| 9. Stage 5 — `dynamic` blocks, `count`, `for_each` | ~2000+ | a major Scanner + Generator expansion |

The previous session's last response named items 1–4 as the immediate
post-MVP candidates. Pick whichever your operator prioritises.

---

## Appendix — Key file index for the next Claude session

When Claude asks "where is X?", these are the canonical answers:

| Concept | Path |
|---|---|
| The constitution | `CONSTITUTION.md` |
| The architecture plan | `terrashift_plan.md` |
| The Stakpak seam mapping | `TERRASHIFT_MAPPING.md` |
| Operating instructions for Claude | `CLAUDE.md` |
| Env-specific decisions | `pre-flight.md` |
| The full onboarding | `docs/onboarding/terrashift-mvp-onboarding.md` |
| This handover | `docs/onboarding/CLAUDE-HANDOVER.md` |
| Implementation prompts library | `terrashift_prompts.md` |
| All feature specs | `specs/*/` |
| Spec Kit infrastructure | `.specify/` |
| Claude Code workspace | `.claude/` |
| Knowledge service | `libs/knowledge/src/knowledge_service.rs` |
| Mapper orchestrator | `libs/engine/src/mapper/mod.rs` |
| Generator templates | `libs/engine/src/generator/templates.rs` |
| CLI entry | `cli/src/main.rs` |
| TUI entry | `tui/src/event_loop.rs` |
| The 173 bundled schemas | `libs/knowledge/seed/<provider>/<category>/*.json` |
| The end-to-end fixture | `fixtures/aws-to-azure-real/` (set up via `scripts/setup-fixtures.ps1`) |

---

**Welcome. Run `claude` from the repo root, paste the prompt in
section 7, and ask the new session to summarise the MVP scorecard
before proposing your first PR.**
