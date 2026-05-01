# Terrashift — Pre-flight Decisions

Environment-specific decisions for running v5 on this machine. v5 was authored
assuming Linux/macOS paths and conventions; this file records the local
adaptations so prompts can run as written.

**Last updated:** 2026-05-02
**Host:** Windows 11 Home (10.0.26200), PowerShell 5.1 + Git Bash 2.41
**Owner:** mohamed.ashraf.gouda99@gmail.com

---

## Decision 1 — Workspace location

**v5 reference:** `~/projects/terrashift/`
**Local reality:** `C:\Users\goda\Desktop\terrashift\`

Sibling to `C:\Users\goda\Desktop\agent\` (which is the Stakpak source).
Keeping them as siblings (rather than nesting) avoids cargo workspace
confusion — Stakpak has its own `Cargo.toml` at its root.

---

## Decision 2 — Reference codebase paths

**Decision:** refs/ lives **inside the workspace** at `terrashift/refs/`,
not at `refs/` (home directory) as v5 originally suggests. This keeps the
project self-contained: anything Opus needs to read at the workspace level
sits in or under `terrashift/`.

**Layout** (all under `C:\Users\goda\Desktop\terrashift\refs\`):

| Path | Type | Target |
|---|---|---|
| `refs/stakpak/` | Directory junction | `C:\Users\goda\Desktop\agent\` |
| `refs/claude-code/` | Directory junction | `C:\Users\goda\Desktop\agent\ClaudeCode-CLI-Src\` |
| `refs/stakpak_arch.md` | File copy (~169 KB) | `C:\Users\goda\Desktop\agent\stakpak_arch.md` |

**Naming convention used everywhere:** `refs/...` (workspace-relative).
Files that originally said `refs/...` from v5 have been updated to use
`refs/...` instead. When new docs or prompts get written, **always use
`refs/...`** — never the home-directory form.

**Junctions vs symlinks:** directory junctions don't require admin on Windows
and are transparent to file readers. A junction looks like a real directory
to `ls`, `Read`, MCP filesystem servers, and any Rust file IO. The underlying
source folders remain single-source-of-truth — updates to `agent/` reflect in
`refs/stakpak/` instantly.

**Not committed:** `/refs/` is in `.gitignore`. Each developer runs
`scripts/setup-refs.ps1` (see below) on first checkout. This avoids:
- Storing 169 KB binary copy in git history
- Junctions don't survive git serialization anyway
- Each contributor may have their own paths to source repos

### Setup script

A new contributor runs:

```powershell
cd C:\Users\goda\Desktop\terrashift
.\scripts\setup-refs.ps1
```

This creates the junctions and copies `stakpak_arch.md`. Source paths are
configurable inside the script if Stakpak / Claude Code source live elsewhere.

### MCP server wiring

`.claude/mcp.json` `filesystem-readonly-refs` server points to the workspace-
local refs/ via absolute paths:

```
C:\Users\goda\Desktop\terrashift\refs\stakpak
C:\Users\goda\Desktop\terrashift\refs\claude-code
C:\Users\goda\Desktop\terrashift\refs
```

### Subdirectories to skip in `refs/claude-code/`

Per terrashift_plan.md §16.2 ("what we don't adopt from Claude Code"):

- `voice/`, `vim/`, `buddy/`, `assistant/`, `moreright/`, `native-ts/`
- `outputStyles/` (Claude Code product surface, not pattern)

The `reference-explorer` sub-agent (`.claude/agents/reference-explorer.md`)
should be told to exclude these from any walkthrough.

---

## Decision 3 — Toolchain version

**v5 P-01:** `rust-toolchain.toml` pin to **1.75** "for ecosystem compatibility"
**This workspace:** **1.94.1** (matches Stakpak's pin)

Rationale for deviation:

1. **v5's reasoning is backwards.** Pinning *lower* doesn't improve compatibility;
   it makes it worse, because newer crates raise their MSRV over time.
2. **Workspace dependencies are recent.** `rmcp 0.11`, `lancedb 0.10`, `sqlx 0.8`,
   `ratatui 0.29` are all 2025/2026-era crates. Their MSRV is likely above 1.75.
3. **Stakpak compiles on 1.94.1** with many of the same dependencies. Using the
   same pin is the lower-risk choice for Stage 1.

Documented as Article XI amendment to v5/P-01.

---

## Decision 4 — Naming: REFERENCES.md vs TERRASHIFT_MAPPING.md

**v5 mixes both names.** Picked: **TERRASHIFT_MAPPING.md** (newer name from P-00).
Confirmed in:
- `.claude/settings.json` `auto_load`
- `CLAUDE.md` reading order

If anything in v5 still says `REFERENCES.md`, treat it as referring to
`TERRASHIFT_MAPPING.md`.

---

## Decision 5 — Stack: docx microservices content is stale

**v5/Terrashift_Plan.docx** §4.1 (microservices), §6.5 (Python LangGraph), §21.1
(Cred Broker microservice in Go) describe an earlier polyglot architecture that
was abandoned.

**Authoritative stack:** all-Rust, single-binary, SQLite + LanceDB embedded.
Source of truth: `terrashift_plan.md`, `CONSTITUTION.md`, `terrashift_prompts.md`,
`terrashift_diagrams.html`.

When the .docx contradicts those files, the .md set wins.

---

## Decision 6 — Host: Windows + Git Bash

**v5 hooks** (`.claude/hooks/pre-commit.sh`, `pre-push.sh`) are bash.

**This workspace:** hooks run via Git Bash (`C:\Program Files\Git\bin\bash.exe`).
Spec Kit's PowerShell scripts run via PowerShell directly.

Two execution paths:
- Bash via Git Bash for `*.sh` files (Stakpak-derived patterns, hooks)
- PowerShell for Spec Kit operations (`.specify/scripts/powershell/`)

Both work concurrently — no conflict.

---

## Decision 7 — Spec Kit integration

**Status:** `.specify/` is initialized at `C:\Users\goda\Desktop\agent\.specify\`
(in the parent agent/ directory). Decision pending: should Spec Kit move into
the new `terrashift/` workspace?

**Recommendation:** initialize a fresh `.specify/` inside `C:\Users\goda\Desktop\terrashift\`
so feature specs (`specs/NNN-foo/`) land alongside the code they describe.

Run from `C:\Users\goda\Desktop\terrashift\`:
```powershell
specify init   # or copy .specify/ from the agent/ workspace
```

Then port `terrashift/CONSTITUTION.md` → `terrashift/.specify/memory/constitution.md`.

---

## Outstanding pre-flight items

| # | Item | Status |
|---|---|---|
| 1 | Reference paths | ✅ Resolved |
| 2 | Workspace location | ✅ Resolved (`C:\Users\goda\Desktop\terrashift\`) |
| 3 | Toolchain | ✅ Resolved (`1.94.1`) |
| 4 | TERRASHIFT_MAPPING.md naming | ✅ Resolved |
| 5 | Stale .docx content | ✅ Resolved (md set is authoritative) |
| 6 | Hooks language | ✅ Resolved (Git Bash for `.sh`, PS for Spec Kit) |
| 7 | Spec Kit location | ⏳ Pending — recommend fresh init in this workspace |
| 8 | **Rust install** | ⏳ **BLOCKING** — `rustc`/`cargo`/`rustup` not on PATH |

### Item 8 — Install Rust

Required before `cargo check --all-targets` will work. Install via:

```powershell
# Recommended: rustup-init (Windows installer)
# https://rustup.rs/  →  download rustup-init.exe  →  run interactively

# Or in Git Bash:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# After install, restart shell, then:
rustup toolchain install 1.94.1
rustup default 1.94.1
rustup component add rustfmt clippy
```

Then run from this workspace:
```powershell
cargo check --all-targets
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
```

---

## Files generated by this setup

| File | Source | Purpose |
|---|---|---|
| `Cargo.toml` | P-01 + this pre-flight | Workspace manifest with 15 members |
| `rust-toolchain.toml` | This pre-flight | Pin to 1.94.1 (deviation from P-01's 1.75) |
| `.gitignore` | P-01 | Rust standard + Terrashift-specific |
| `README.md` | P-01 | One-paragraph + doc links |
| `ATTRIBUTIONS.md` | P-01 | Stakpak + Claude Code attribution per Article II |
| `CLAUDE.md` | terrashift_opus_setup.md §3 | Top-level Opus instructions |
| `CONSTITUTION.md` | Copied from v5 | 13 articles |
| `TERRASHIFT_MAPPING.md` | P-00 placeholder | To be filled by P-00 |
| `terrashift_plan.md` | Copied from v5 | Architecture |
| `terrashift_prompts.md` | Copied from v5 | P-00..P-22 templates |
| `pre-flight.md` | This setup | THIS file |
| 15 × `Cargo.toml` per crate | P-01 | Crate skeletons |
| 15 × `lib.rs`/`main.rs` per crate | P-01 | Empty entry points |
| `.github/workflows/ci.yml` | P-01 | fmt + clippy + test on every PR |
| `.claude/settings.json` | terrashift_opus_setup.md §9 | Project Claude config |
| `.claude/agents/*.md` (5) | terrashift_opus_setup.md §4 | Sub-agents |
| `.claude/commands/*.md` (5) | terrashift_opus_setup.md §5 | Slash commands |
| `.claude/skills/*.md` (3) | terrashift_opus_setup.md §7 | Skills |
| `.claude/mcp.json` | terrashift_opus_setup.md §6 | MCP server registry |
| `.claude/hooks/*.sh` (2) | terrashift_opus_setup.md §8 | Pre-commit + pre-push |
| `docs/terrashift_opus_setup.md` | Copied | Full Opus setup spec |
| `docs/terrashift_diagrams.html` | Copied | 9 architecture diagrams |
| `docs/Terrashift_Plan.docx` | Copied | Longform master (some sections stale; see Decision 5) |
| `docs/speckit_commands.txt` | Copied | Spec Kit execution runbook |
