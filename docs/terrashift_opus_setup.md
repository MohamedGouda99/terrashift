# Terrashift — Opus 4.7 Setup

How Opus 4.7 is configured to develop Terrashift. Practical handbook, not a manifesto.

**You are running Claude Code locally on your machine.** This document describes the `.claude/` directory layout that ships with the Terrashift workspace, the sub-agents available, the MCP servers wired up, and the slash commands the team actually uses.

The goal: any teammate can clone `terrashift`, run `claude` in the directory, and have the same working environment as everyone else.

---

## 1. The big picture

Three things shape how Opus 4.7 works on Terrashift:

1. **Reference codebases on disk.** Both `~/refs/stakpak` and `~/refs/claude-code` are checked out locally. Opus reads from them constantly. Most prompts in `terrashift_prompts.md` start with "read X from `~/refs/stakpak/...`."

2. **The `.claude/` directory in the workspace.** This is the project-specific configuration that ships with the repo. Sub-agents, slash commands, MCP servers, hooks, skills — all defined here, all version-controlled.

3. **The constitution + prompts library.** The constitution defines *what we build*; the prompts library defines *how we direct Opus to build it*. Both live in the workspace root.

That's the whole system. No proprietary tooling, no SaaS dependency, no hidden config.

---

## 2. Directory layout

```
terrashift/                          # the workspace
├── .claude/                         # Claude Code config (committed to repo)
│   ├── settings.json                # project-level settings
│   ├── agents/                      # sub-agent definitions
│   │   ├── code-reviewer.md
│   │   ├── constitution-checker.md
│   │   ├── eval-runner.md
│   │   ├── security-auditor.md
│   │   └── reference-explorer.md
│   ├── commands/                    # slash commands
│   │   ├── plan.md
│   │   ├── stage-gate.md
│   │   ├── token-audit.md
│   │   ├── article.md
│   │   └── refchk.md
│   ├── skills/                      # reusable skill definitions
│   │   ├── rust-tool-impl.md
│   │   ├── stakpak-pattern.md
│   │   └── eval-design.md
│   ├── mcp.json                     # MCP server registry
│   └── hooks/                       # lifecycle hooks
│       ├── pre-commit.sh
│       └── pre-push.sh
├── CLAUDE.md                        # top-level instructions for Opus
├── CONSTITUTION.md                  # 13 articles
├── TERRASHIFT_MAPPING.md            # output from P-00 — Stakpak seams → Terrashift
├── terrashift_plan.md
├── terrashift_prompts.md
├── ATTRIBUTIONS.md
├── Cargo.toml
├── cli/
├── tui/
└── libs/...

External (not in workspace, but accessible via MCP):
├── ~/refs/stakpak_arch.md           # PRIMARY architectural reference (~2,840 lines)
├── ~/refs/stakpak/                  # Stakpak source code
└── ~/refs/claude-code/              # Claude Code TypeScript source
```

Three files are the entry points Opus reads on every session:

- **`CLAUDE.md`** — top-level instructions, written for Opus
- **`CONSTITUTION.md`** — the rules Opus must follow (13 articles)
- **`TERRASHIFT_MAPPING.md`** — the seam-to-Terrashift mapping (output of P-00). Replaces the older REFERENCES.md model: instead of summarizing what's in `~/refs/`, this file lists the explicit mapping from Stakpak's seams (per `stakpak_arch.md section 39`) to Terrashift's crates, plus citations to which `stakpak_arch.md` sections inform each Terrashift component.

Everything else is optional context Opus pulls in as needed. The architecture document at `~/refs/stakpak_arch.md` is accessible via the `filesystem-readonly-refs` MCP server and read on demand for specific patterns.

---

## 3. CLAUDE.md — top-level instructions

This is the first file Opus reads. It's short by design.

```markdown
# Terrashift — operating instructions for Claude

You are Opus 4.7 working on Terrashift. Read these files in order before
acting on any prompt:

1. CONSTITUTION.md — the rules. Cite articles in PR descriptions. (13 articles total.)
2. TERRASHIFT_MAPPING.md — the seam-to-Terrashift mapping (output of P-00).
3. terrashift_plan.md — what we're building.

Stack: Rust everywhere. `stakai` for LLM, `rmcp` for MCP, `lancedb` for
vectors, `hcl-rs` for HCL2. Single binary distribution. No Python in
the production stack.

Reference codebases at ~/refs/:
- ~/refs/stakpak_arch.md — PRIMARY architectural reference (~2,840 lines)
- ~/refs/stakpak/ — Stakpak source code (Apache 2.0); ground truth
- ~/refs/claude-code/ — Claude Code TypeScript (secondary, agent-loop concepts)

When implementing anything: cite the stakpak_arch.md section number you
patterned after (e.g., "stakpak_arch.md section 8" for the agent kernel). Read the
architecture document first; descend into Stakpak source only when verifying
a specific file:line citation.

When deviating from Stakpak's pattern, say why and which constitution
article governs the deviation.

When unsure, ask. When confident, ship. When something will fail
loudly later, surface it now.
```

That's it. One screen of text. Anything longer becomes wallpaper Opus stops reading.

---

## 4. Sub-agents

Five sub-agents. Each is a focused worker that handles one kind of task.

### `code-reviewer`

Used by the lead before merging any PR. Reads the diff and produces a structured review.

```yaml
# .claude/agents/code-reviewer.md
name: code-reviewer
description: Reviews a PR diff against the constitution and reference codebases.
tools: [Read, Grep, Glob, Bash]
prompt: |
  You are reviewing a PR for Terrashift. For the diff in the current
  branch:

  1. Identify which constitution articles are touched.
  2. For each touched article, verify compliance.
  3. Compare any new patterns against ~/refs/stakpak — if Stakpak does
     this differently, note it and ask whether the deviation is intentional.
  4. Check test coverage. If a public function lacks tests, flag it.
  5. Run `cargo clippy --all-targets -- -D warnings` and report any issues.

  Output format:
  - APPROVE / REQUEST CHANGES / COMMENT
  - Constitution articles touched: [list]
  - Compliance findings: [list]
  - Suggestions: [list]

  Do not modify code. Review only.
```

### `constitution-checker`

Runs as a pre-commit hook. Lightweight scan for the most common violations.

```yaml
# .claude/agents/constitution-checker.md
name: constitution-checker
description: Quick check that staged changes don't violate any constitution article.
tools: [Read, Bash]
prompt: |
  Read the staged diff (git diff --cached). For each file:

  - Article I: any new component looping over LLM calls? If yes, is it
    in Recovery, Cost Optimizer, or Cutover? If not, flag it.
  - Article III: any LLM-emitted attribute that bypasses the Validator?
  - Article V: any credential held in process memory beyond the
    operation that needs it? Any LLM context containing a raw secret?
  - Article X: any new code path without a tracing span?
  - Article XII: any new prompt without a documented cost upper bound?

  Output: PASS or list of violations with file:line references.
  Do not modify code.
```

### `eval-runner`

Triggered when an eval is requested or a regression is suspected.

```yaml
# .claude/agents/eval-runner.md
name: eval-runner
description: Runs the golden-migrations eval suite and reports results.
tools: [Bash, Read]
prompt: |
  Run the Terrashift eval suite:

  1. cargo test --package terrashift-eval --release
  2. Capture token-cost output for each golden migration
  3. Compare against the baseline in eval-baseline.json
  4. Flag any migration with >30% cost increase or any test failure

  Output:
  - Pass/fail count
  - Token cost delta vs baseline (per migration + total)
  - Wall-clock time delta vs baseline
  - Recommendation: SAFE TO MERGE / INVESTIGATE / BLOCK

  This agent does not modify code.
```

### `security-auditor`

Run on demand before any release or stage gate.

```yaml
# .claude/agents/security-auditor.md
name: security-auditor
description: Audits the codebase for credential handling and secret leakage.
tools: [Read, Grep, Bash]
prompt: |
  Audit the Terrashift codebase against Article V:

  1. Grep for any hard-coded credential patterns (AWS access keys,
     GCP service account JSON shapes, Azure connection strings).
  2. Trace credential flow: where credentials enter the process,
     where they are passed, where they are dropped.
  3. Verify zeroize is used on every credential type.
  4. Verify the LLM context never contains a resolved secret value.
  5. Check that every credential operation has a corresponding
     audit log entry.

  Output: a markdown report with findings, severity (high/med/low),
  and suggested fixes. No code changes — review only.
```

### `reference-explorer`

The "go read X from the references" agent. Most useful when starting a new component. Now updated to use `stakpak_arch.md` as the primary entry point — it reads the architecture document first and only descends into Stakpak source when a specific file:line citation needs verification.

```yaml
# .claude/agents/reference-explorer.md
name: reference-explorer
description: Reads a specific pattern from stakpak_arch.md or the source codebases and summarizes it.
tools: [Read, Grep, Glob]
prompt: |
  You will be asked to find a specific pattern. Order of operations:

  1. PRIMARY: Read the relevant section(s) of `~/refs/stakpak_arch.md` first.
     The architecture doc is organized into 8 parts and 42+ numbered sections.
     Common section references:
     - section 8 — agent-core kernel (Tool trait, agent loop, approval FSM, context reduction)
     - section 9 — shared and api crates (message types, session storage)
     - section 10 — stakai LLM SDK
     - section 13 — MCP suite
     - section 16 — TUI
     - section 22 — checkpoint and resume
     - section 23 — context trimming with cache preservation
     - section 27 — secret detection / redaction
     - section 28 — reversible file operations
     - section 29 — Warden sandbox
     - section 30 — shell command-level approvals
     - section 39 — the 11 seams (with effort estimates per swap)
     - section 40 — domain artifacts to replace
     - section 41 — phased mirroring sequence
     - section 42 — anti-patterns

  2. SECONDARY: If the architecture doc references a specific file:line in
     `~/refs/stakpak/`, descend into that file and verify the doc's
     description matches the actual code. Note any discrepancies.

  3. TERTIARY: For agent-loop concepts, also check `~/refs/claude-code/` —
     the Tool.ts, QueryEngine.ts, query.ts, commands/ directory, services/compact/
     are the most useful files.

  For each request:
  1. Locate the relevant section(s) in stakpak_arch.md.
  2. Summarize the pattern in 3-5 bullets.
  3. Quote the most important 5-10 lines from the architecture doc OR the
     source code (cite which one).
  4. Identify what would need to change to apply the pattern to Terrashift's needs.
  5. Note which constitution article(s) the pattern relates to (Articles I-XIII).

  Do not write Terrashift code. Output is a summary document used to inform a
  subsequent implementation prompt.
```

---

## 5. Slash commands

Five slash commands. These are typed in the Claude Code chat, like `/plan`. Each is backed by a markdown file.

### `/plan`

Opens `terrashift_plan.md` and lets you ask questions about it without re-pasting.

```markdown
# .claude/commands/plan.md
Read terrashift_plan.md. Answer questions about the architecture,
stages, or constitution. Quote the relevant section when responding.
If the question requires reading reference codebases, do that.
Do not modify the plan unless explicitly asked.
```

### `/stage-gate`

Runs a stage-gate review using P-16 from the prompts library.

```markdown
# .claude/commands/stage-gate.md
Run the stage-gate review per prompt P-16. Before producing the
review, confirm with the user which stage we are gating (1, 2, etc.).

Read the gate criteria from terrashift_plan.md section 17. For each:
PASS / FAIL / PARTIAL with evidence. End with go/no-go recommendation.
```

### `/token-audit`

Runs the token-cost analysis from P-19.

```markdown
# .claude/commands/token-audit.md
Run a token cost audit. Pull the latest eval-runner output. Apply
Article XII rules in priority:
1. Cache hit rate (per prompt)
2. Tier routing decisions (cheap/balanced/premium)
3. Context window sizes
4. Whether pure-code path was attempted before LLM call

Output: top-5 most expensive prompts, with optimization suggestions.
```

### `/article`

Look up a specific constitution article and explain it in context.

```markdown
# .claude/commands/article.md
Look up the requested constitution article in CONSTITUTION.md.
Explain it. Then list 2-3 places in the codebase where this article
applies, with file:line references. End with examples of compliance
and non-compliance.
```

### `/refchk`

Cross-check a proposed implementation against the reference codebases.

```markdown
# .claude/commands/refchk.md
For the file or function the user references: find the equivalent
pattern in ~/refs/stakpak (and ~/refs/claude-code if relevant).
Compare. Highlight any meaningful differences. Suggest whether
Terrashift's approach is consistent with the reference or whether
a deviation should be documented.
```

---

## 6. MCP servers

Three MCP servers wired into the development environment. Defined in `.claude/mcp.json`.

```json
{
  "mcpServers": {
    "terrashift-knowledge": {
      "command": "cargo",
      "args": ["run", "--package", "terrashift-knowledge-mcp"],
      "description": "Local provider schema and mapping corpus, accessible as MCP tools"
    },
    "stakpak-reference": {
      "command": "stakpak",
      "args": ["mcp", "start", "--tool-mode", "local"],
      "description": "Stakpak's local MCP tools — used as reference for our own MCP design"
    },
    "filesystem-readonly-refs": {
      "command": "npx",
      "args": [
        "@modelcontextprotocol/server-filesystem",
        "/home/<user>/refs/stakpak",
        "/home/<user>/refs/claude-code",
        "/home/<user>/refs"
      ],
      "description": "Read-only access to reference codebases AND the stakpak_arch.md document at ~/refs/stakpak_arch.md"
    }
  }
}
```

**What each one does:**

- **`terrashift-knowledge`** is our own crate that exposes the schema cache and mapping corpus as MCP tools. Built in P-07 and beyond. Lets Opus query "is `aws_s3_bucket.acl` valid in provider v5.30?" without us constructing the prompt manually.

- **`stakpak-reference`** runs the actual Stakpak binary in MCP mode. Lets us directly compare our patterns against theirs. This is the most novel use of an MCP server in the setup — we're using a working production tool as a reference oracle.

- **`filesystem-readonly-refs`** mounts both reference codebases AND the parent `~/refs/` directory (which contains `stakpak_arch.md`) as read-only filesystems. Opus can do directory walks and grep without shell access. The architecture document is the primary entry point — Opus reads sections of `stakpak_arch.md` first, then descends into source code when the doc references a specific file:line that needs verification.

When you ask Opus to "implement X patterned after stakpak_arch.md sectionN," it can read the relevant section directly via `filesystem-readonly-refs`, then optionally call `stakpak-reference`'s MCP tools or read source via the same filesystem mount to verify. Three channels (architecture doc, live MCP, source code), one workflow.

---

## 7. Skills

Three skills. Reusable behavior bundles loaded on demand.

### `rust-tool-impl`

Loaded whenever you're implementing a new Tool.

```markdown
# .claude/skills/rust-tool-impl.md
When implementing a Rust Tool for Terrashift:

1. Read libs/agent-core/src/tool.rs to understand the trait.
2. Read ~/refs/stakpak/libs/agent-core/ for an analogous tool.
3. Define your tool's input/output structs with serde + schemars.
4. Implement the trait async.
5. Write tests covering: happy path, malformed input, error from
   downstream call, schema validation.
6. Add the tool to the registry in libs/agent-core/src/registry.rs.
7. Cite Article I in PR description.
```

### `stakpak-pattern`

Loaded when porting a specific Stakpak pattern.

```markdown
# .claude/skills/stakpak-pattern.md
When porting a Stakpak pattern to Terrashift:

1. Read the Stakpak source carefully. Understand it before you copy it.
2. Identify what changes for Terrashift (different domain, different
   user, different scope).
3. Adapt the pattern, don't transliterate it. Lines-of-code count
   doesn't have to match.
4. Cite the source files in code comments.
5. Note the deviation in PR description with reasoning.
6. If the deviation is significant, propose adding it to ATTRIBUTIONS.md.
```

### `eval-design`

Loaded when designing a new golden migration or eval.

```markdown
# .claude/skills/eval-design.md
When designing a new eval:

1. Source TF must be realistic but minimal. ~5-15 resources.
2. Expected target TF is hand-curated and reviewed by a teammate.
3. Document which constitution article(s) this eval validates.
4. Document the expected token-cost ceiling.
5. Document the wall-clock time ceiling.
6. Add to terrashift-evals/README.md with full provenance.
7. Cite Article III in PR description.
```

---

## 8. Hooks

Two lifecycle hooks. Both run automatically; both are bypassable in emergencies via `--no-verify`.

### `pre-commit.sh`

Runs the constitution-checker sub-agent on staged changes.

```bash
#!/bin/bash
# .claude/hooks/pre-commit.sh
set -e
echo "Running constitution-checker..."
claude run-agent constitution-checker --staged
```

### `pre-push.sh`

Runs the eval-runner sub-agent before push to main.

```bash
#!/bin/bash
# .claude/hooks/pre-push.sh
set -e
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" = "main" ]; then
  echo "Running eval-runner before push to main..."
  claude run-agent eval-runner
fi
```

---

## 9. Settings

Project-level settings in `.claude/settings.json`:

```json
{
  "model": "claude-opus-4-7",
  "fallback_model": "claude-sonnet-4-7",
  "max_tokens_per_session": 200000,
  "auto_load": [
    "CLAUDE.md",
    "CONSTITUTION.md",
    "TERRASHIFT_MAPPING.md"
  ],
  "permissions": {
    "tools": {
      "Bash": "ask",
      "Edit": "ask",
      "Write": "ask",
      "Read": "always",
      "Grep": "always",
      "Glob": "always"
    },
    "subagents": "enabled"
  },
  "tracing": {
    "enabled": true,
    "destination": "~/.terrashift/traces"
  }
}
```

Three things worth noting:

- **`auto_load`** means CLAUDE.md, CONSTITUTION.md, and TERRASHIFT_MAPPING.md are read at the start of every session. Opus always has the rules and the seam mapping in context.
- **`permissions.tools.Bash: "ask"`** means every shell command requires confirmation. This matters because Opus might run `cargo apply-real-credentials-to-prod`. Ask first.
- **`subagents: "enabled"`** is the equivalent of Stakpak's `--enable-subagents` flag (per `stakpak_arch.md section 3` and the subagent permission model). Required for the five sub-agents above to dispatch.

---

## 10. How to actually use this

**Day-one (per teammate):**

1. Clone `terrashift`. Clone `~/refs/stakpak` and `~/refs/claude-code` separately.
2. Run `claude` in the terrashift directory.
3. Type `/refchk Cargo.toml` — confirms Opus can read both reference codebases.
4. Run prompt P-00 from the prompts library. This produces `REFERENCES.md`.
5. Run prompt P-01. This sets up the workspace.

**Daily:**

- Pick a prompt from `terrashift_prompts.md` (P-02 through P-16 cover Stage 1).
- Paste into Claude Code.
- Substitute placeholders.
- Review the output before merging.

**Before merging any PR:**

- The pre-commit hook runs `constitution-checker` automatically.
- Manually invoke `code-reviewer` for non-trivial changes.

**Before pushing to main:**

- The pre-push hook runs `eval-runner` automatically.
- For pre-stage-gate pushes, manually invoke `security-auditor`.

**At stage gates:**

- Run `/stage-gate`. This runs the gate review per P-16.
- All findings get a ticket. No partials get pushed past.

---

## 11. What this setup does NOT do

Worth being explicit about the boundaries:

- **No autopilot mode.** Every shell command, file write, and credential operation requires explicit confirmation. We're building production software, not letting an agent run wild.
- **No auto-merge.** Opus produces PRs; humans review and merge.
- **No production credentials in this setup.** The credential broker (P-10) is the only path to credentials. Opus operates against staging or sandbox accounts.
- **No proprietary observability.** Tracing goes to local files by default. Add OpenTelemetry export when we have a stack to send it to.

---

## 12. Updating this setup

The `.claude/` directory is committed to the repo. Changes to it are PRs like any other. They should:

1. Cite which constitution article they touch (usually Article XI — Amendments).
2. Be reviewed by the team before merging.
3. Update this document if the change is structural.

Sub-agent and slash command additions are encouraged. Each one earns its keep by handling a task we'd otherwise prompt manually for.

---

## 13. Quick reference card

**Sub-agents:**
- `code-reviewer` — PR review against constitution
- `constitution-checker` — pre-commit lightweight scan
- `eval-runner` — runs golden migrations
- `security-auditor` — credential and secret audit
- `reference-explorer` — read patterns from Stakpak / Claude Code

**Slash commands:**
- `/plan` — query the architecture plan
- `/stage-gate` — stage-gate review (P-16)
- `/token-audit` — token cost audit (P-19)
- `/article` — look up a constitution article
- `/refchk` — cross-check against references

**MCP servers:**
- `terrashift-knowledge` — our schema cache + mapping corpus
- `stakpak-reference` — Stakpak running in MCP mode for reference
- `filesystem-readonly-refs` — read-only mount of both reference repos

**Skills:**
- `rust-tool-impl` — implementing a new Tool
- `stakpak-pattern` — porting a Stakpak pattern
- `eval-design` — designing a golden migration

**Hooks:**
- `pre-commit` — runs `constitution-checker`
- `pre-push` — runs `eval-runner` if pushing to main

**Always-loaded files:**
- `CLAUDE.md`, `CONSTITUTION.md`, `REFERENCES.md`
