# Contributing to Terrashift

Thanks for your interest. Terrashift is in active stage-gated development —
read these first before opening a PR.

## Required reading (in order)

1. **[CONSTITUTION.md](CONSTITUTION.md)** — 13 articles. Cite articles in PR
   descriptions. Article XIII has 10 source-derived anti-pattern rules.
2. **[CLAUDE.md](CLAUDE.md)** — project operating instructions (also used by
   the Claude Code agent that does most of the work).
3. **[TERRASHIFT_MAPPING.md](TERRASHIFT_MAPPING.md)** — Stakpak seam →
   Terrashift mapping.
4. **[terrashift_plan.md](terrashift_plan.md)** — what we're building.
5. **[SESSION_PLAN.md](SESSION_PLAN.md)** — multi-session ladder (S1 → S37).

## Stack discipline

Rust everywhere. We use:

- `stakai` 0.3 — LLM SDK (Stakpak-published)
- `rmcp` — Model Context Protocol
- `lancedb` — vector DB (Stage 3+)
- `hcl-rs` — HCL2 emit
- `sqlx` — SQLite-backed audit + knowledge cache
- `tree-sitter-bash` — shell-command-level approval parsing

**No Python in the production stack.** No LangChain, no LangGraph, no LiteLLM.

## Workflow

We use Spec Kit:

```
/speckit-git-feature → /speckit-specify → /speckit-clarify
  → /speckit-plan → /speckit-tasks → /speckit-implement
  → /speckit-analyze → /speckit-checklist → /speckit-git-commit
```

Never skip `/speckit-clarify` — that's where ambiguities surface.

## Local checks before PR

```sh
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo test --package terrashift-eval --release   # if touching LLM path
```

The pre-commit hook in this repo runs `cargo fmt` automatically.

## PR description requirements

Every PR description must include the three sections from
`.github/PULL_REQUEST_TEMPLATE.md`:

```markdown
## Constitution
- Article N (rationale)
- Article XIII rule M (where applicable)

## stakpak_arch.md
- section N (what was mirrored)

## Eval impact
- Token cost delta vs baseline (per Article XII rule 4)
```

## Commit message conventions

We use Conventional Commits with these scopes:

- `feat(p-NN)` — new feature implementing prompt P-NN
- `feat(r-NN)` — new feature implementing remediation item R-NN
- `refactor(p-NN)` — structural refactor of prompt P-NN scope
- `fix(p-NN)` — bug fix
- `chore(deps)` / `chore(ci)` — dependency / CI hygiene
- `docs(...)` — documentation only

Co-author with Claude when applicable:

```
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
```

## Adding a new agent

Per Constitution Article I, adding a new agent **requires an RFC + stage-gate
approval**. Open a feature-request issue with kind = "New agent" and link
the RFC discussion.

Stage 1 has zero agents. Stage 2 introduces Recovery + Cost Optimizer.

## License

By contributing, you agree your contribution is licensed under
[Apache 2.0](LICENSE).
