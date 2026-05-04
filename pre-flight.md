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

| Path | Type | Source |
|---|---|---|
| `refs/stakpak/` | **Fresh git clone** of `https://github.com/stakpak/agent.git` | upstream main |
| `refs/claude-code/` | Directory junction | `C:\Users\goda\Desktop\agent\ClaudeCode-CLI-Src\` |
| `refs/stakpak_arch.md` | File copy (~169 KB) | `C:\Users\goda\Desktop\agent\stakpak_arch.md` |

**Why clone for stakpak (not junction)?** The user's local working copy of
the Stakpak repo at `C:\Users\goda\Desktop\agent\` contains unrelated
additions (terrashift_v5/, ClaudeCode-CLI-Src/, .claude/, .specify/,
stakpak_arch.md, etc.). A junction surfaces all of those, polluting any
"read patterns from Stakpak" prompt. A fresh clone gives a clean,
upstream-only mirror — exactly the contents of the public repo, nothing
more. Trade-off: ~50-200 MB on disk + a network operation on first setup.

**Why junction for claude-code (not clone)?** The local ClaudeCode-CLI-Src/
folder is already isolated to Claude Code source only. No pollution to
filter out. Junction is faster + zero-duplicate.

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

### Item 8 — Install Rust + a working toolchain (✅ resolved on this machine)

**Verified-working solution on this machine (2026-05-02):**

The combination that compiles all 15 crates cleanly on Windows without admin:

| Component | Version | Location |
|---|---|---|
| Rust toolchain | `1.94.1-x86_64-pc-windows-gnullvm` | `~/.rustup/toolchains/...` |
| LLVM-MinGW (clang + lld + dlltool + libunwind + compiler-rt) | latest UCRT release | `~/llvm-mingw/` |
| Workspace override | `rustup override set 1.94.1-x86_64-pc-windows-gnullvm` | per-workspace state |

**Three-line setup for fresh checkouts:**

```powershell
cd C:\path\to\terrashift
.\scripts\setup-toolchain.ps1   # idempotent; installs everything above
cargo check --all-targets        # ~5-15 min first time
```

The script installs:
1. LLVM-MinGW UCRT (~180 MB) into `~/llvm-mingw/` and adds bin to user PATH
2. Rust `1.94.1-x86_64-pc-windows-gnullvm` toolchain (~200 MB) via rustup
3. Sets the workspace override so cargo uses the matching pair

#### Why this combination

The Rust-on-Windows linker landscape has three options, only one of which works
without admin and gives a successful `cargo check`:

| Toolchain | Linker stack | Works without admin? | Verdict |
|---|---|---|---|
| `*-msvc` | Microsoft `link.exe` + VC++ runtime | ❌ needs VS Build Tools (~6 GB, admin) | viable but heavy |
| `*-gnu` | GCC's `ld` + libgcc/libstdc++ | requires real MinGW-w64 GCC | clashes with LLVM-MinGW (`-lgcc_eh` not found) |
| **`*-gnullvm`** | **clang/lld + libunwind/compiler-rt** | ✅ **with LLVM-MinGW** | **what we use** |

The trap: **LLVM-MinGW alone is not enough** — it provides the linker stack, but
without the matching `gnullvm` Rust target, rustc passes GCC-specific link
flags (`-lgcc`, `-lgcc_eh`) that LLVM-MinGW can't satisfy. The two halves must
match.

#### Alternative paths if `setup-toolchain.ps1` doesn't fit

**A. VS Build Tools (heaviest, most standard)**
```powershell
# Admin shell:
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
# Workspace stays on default 1.94.1-msvc — no rustup override needed
cargo check --all-targets
```

**B. Real MinGW-w64 GCC + Rust gnu (not gnullvm)**
```powershell
choco install mingw -y   # admin shell
rustup override set 1.94.1-x86_64-pc-windows-gnu
cargo check --all-targets
```

#### Verify the install worked

```powershell
cd C:\Users\goda\Desktop\terrashift
rustup show active-toolchain   # Should show: 1.94.1-x86_64-pc-windows-gnullvm
cargo check --all-targets       # Should compile all 15 crates
cargo fmt -- --check            # Should produce no diffs
cargo clippy --all-targets -- -D warnings   # Should pass
```

Successful `cargo check` is the green-light gate for starting P-NN work.

---

## Decision 9 — LLM provider for testing

**Picked:** `meta-llama/Llama-3.3-70B-Instruct` via **Groq** (`llama-3.3-70b-versatile`).

| Why this | Rationale |
|---|---|
| Free tier | 30 RPM, 6K TPM, 1K RPD — no credit card required |
| Strict JSON | Native `json_schema` mode → Article III enforcement (no hallucinated attributes) |
| Context | 128K tokens (well above 32K minimum for the Mapper) |
| Quality | IFEval 92.1% (best in class for instruction following on free tier) |
| Speed | ~275 tokens/sec on Groq — Mapper feels instant |
| Tool use | Native function calling for Stage 2 Recovery agent |

**Setup:**
1. Sign up at https://console.groq.com/ (no card required)
2. Get API key
3. Set in `~/.terrashift/config.toml` per decision 10 below:
   ```toml
   [profiles.default.providers.groq]
   type = "openai-compatible"
   api_endpoint = "https://api.groq.com/openai/v1"
   api_key_env = "GROQ_API_KEY"

     [profiles.default.tiers]
     eco = "groq/llama-3.3-70b-versatile"
     smart = "groq/llama-3.3-70b-versatile"  # same model — Stage 1 doesn't need tier split
   ```

Alternatives surveyed (and why not chosen as default):
- **Qwen 2.5 Coder 32B** — better at code but weaker on JSON adherence
- **DeepSeek-R1 / R1-Distill-Llama-70B** — overkill for deterministic mapping; emits `<think>` traces that pollute structured output
- **Mistral Large** — closed-weights, no free tier
- **Hermes 3 70B** — no first-class hosted free endpoint with structured outputs

For embeddings (Stage 3+ RAG): **`BAAI/bge-m3`** via HF Inference API or local `fastembed-rs`.

## Decision 10 — Test fixture (real AWS→Azure migration)

**Picked:** `github.com/PratikMahajan/AWS-to-AZURE-Infrastructure-Migration`

| Field | Value |
|---|---|
| License | **GPL-2.0** ⚠️ — fixture only, never bundled with Terrashift binary |
| Resources | ~30 across 18 AWS modules + 11 Azure modules |
| AWS services | VPC, EC2 + ASG + ALB, RDS (MySQL), DynamoDB, S3, Route53, Lambda, SNS, WAF, CodeDeploy, IAM |
| Azure equivalents | Virtual Network, VM Scale Set + LB, MariaDB, Cosmos DB, Storage Account, Azure DNS, Function, Event Grid |
| Real-world | Deploys an actual webapp with EC2 user-data wiring DB credentials |
| Quality | Modular, consistent naming, but Terraform 0.12-era syntax (needs `terraform 0.13upgrade` + provider modernization) |

**License handling:** GPL-2 fixture is **never committed** to the Apache-2 Terrashift repo. It's cloned into `terrashift/fixtures/aws-to-azure-real/` (gitignored) by `scripts/setup-fixtures.ps1`. Mappings derived from running Terrashift against it are first-party (Apache-2), but the source `.tf` files stay GPL where they live.

**Asymmetry caveat:** Pratik's mapping is partial — 7 AWS modules have no Azure counterpart, 4 Azure modules have no AWS counterpart. Useful as a realistic stress test for the Mapper's "unmappable resource" handling (Article IV — failures must be loud), but not a 1:1 mirror.

**Synthesized fallback:** if GPL-2 ever becomes a blocker, build an Apache-2 fixture from `terraform-aws-modules/*` + `Azure/terraform-azurerm-*` (both Apache-2) covering the same resources.

**How to fetch:**
```powershell
cd C:\Users\goda\Desktop\terrashift
.\scripts\setup-fixtures.ps1
```

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
