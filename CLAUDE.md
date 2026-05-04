# Terrashift — operating instructions for Claude

You are Opus 4.7 (or successor) working on Terrashift. Read these files in
order before acting on any prompt:

1. **CONSTITUTION.md** — the rules. 13 articles. Cite articles in PR descriptions.
   Article XIII enumerates 10 source-derived anti-patterns from Stakpak's history.
2. **TERRASHIFT_MAPPING.md** — Stakpak seam → Terrashift mapping (output of P-00).
   Tells you where each Stakpak pattern lands in this workspace.
3. **terrashift_plan.md** — what we're building.
4. **pre-flight.md** — environment-specific decisions (paths, toolchain, host).

## Stack

Rust everywhere. `stakai` for LLM, `rmcp` for MCP, `lancedb` for vectors,
`hcl-rs` for HCL2. Single binary distribution. **No Python in the production
stack.** No LangChain, no LangGraph, no LiteLLM.

## Reference codebases

- `refs/stakpak_arch.md` (~2,840 lines) — **PRIMARY** architectural reference
- `refs/stakpak/` — Stakpak source code (Apache 2.0); ground truth
- `refs/claude-code/` — Claude Code TypeScript (secondary, agent-loop concepts)

All paths are **workspace-relative** — `refs/` lives at the workspace root.
On a fresh checkout, run `scripts/setup-refs.ps1` to recreate the junctions
and copy `stakpak_arch.md`. See `pre-flight.md` decision 2 for the rationale
and the source-path defaults.

## Citation discipline

When implementing anything, cite the `stakpak_arch.md` section number you
patterned after (e.g., "stakpak_arch.md section 8" for the agent kernel). Read
the architecture document first; descend into Stakpak source only when
verifying a specific file:line citation.

When deviating from Stakpak's pattern, say why and which constitution article
governs the deviation.

## PR description requirements

Every PR description must include three sections:

```markdown
## Constitution
- Article N (rationale)
- Article XIII rule M (where applicable)

## stakpak_arch.md
- section N (what was mirrored)

## Eval impact
- Token cost delta vs baseline (per Article XII rule 4)
```

## Behaviour

When unsure, ask. When confident, ship. When something will fail loudly later,
surface it now (Article IV).

Adding a new agent requires explicit RFC + stage-gate approval (Article I).
Stage 1 has zero agents. Stage 2 introduces Recovery + Cost Optimizer.

## Workflow

Use Spec Kit for the inner loop per `docs/speckit_commands.txt`:

```
/speckit-git-feature → /speckit-specify → /speckit-clarify
  → /speckit-plan → /speckit-tasks → /speckit-implement
  → /speckit-analyze → /speckit-checklist → /speckit-git-commit
```

Never skip `/speckit-clarify`. It's where v5's ambiguities surface — resolving
them before code is 10× cheaper than after.
