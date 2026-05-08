# Contributing to Terrashift

Thanks for your interest. Terrashift is in active stage-gated development —
please read this guide before opening a PR.

## Stack discipline

Rust everywhere:

- `stakai` 0.3 — LLM SDK (Stakpak-published)
- `rmcp` — Model Context Protocol
- `lancedb` — vector DB (Stage 3+)
- `hcl-rs` — HCL2 parser + emitter
- `sqlx` — SQLite-backed audit log + knowledge cache
- `tree-sitter-bash` — shell-command-level approval parsing

**No Python in the production stack.** No LangChain, no LangGraph, no LiteLLM.

## Local checks before PR

```sh
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo test --package terrashift-eval --release   # if touching the LLM path
```

The pre-commit hook in this repo runs `cargo fmt` automatically.

## PR description requirements

Every PR description must include three sections — the
[`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) scaffolds them:

```markdown
## Conventions
- Convention N (rationale — what project rule applies and why)

## Architectural pattern
- Section / file N from the architecture reference (what was mirrored)

## Eval impact
- Token cost delta vs baseline (zero for refactors)
```

The "Conventions" section maps to the project's internal governance rules
(bounded agents, Article III Validator gate, deterministic-where-possible,
loud failure, no `unwrap` / `expect` in production code, etc.). Maintainers
will help reviewers identify which convention applies if you're new.

## Branch naming conventions

Use Conventional-Commits-prefixed slugs. Pattern: `<type>/<slug>` where
`<type>` matches the commit-type vocabulary below and `<slug>` is a
short kebab-case description (≤4 hyphenated words).

| Branch prefix | When to use | Example |
|---|---|---|
| `feat/<slug>` | New user-visible feature | `feat/r07-mvp-closure` |
| `fix/<slug>` | Bug fix on a non-blocking issue | `fix/install-path-windows` |
| `refactor/<slug>` | Pure refactor; no behaviour change | `refactor/scanner-walkdir` |
| `chore/<slug>` | Dependency bumps, CI hygiene, repo plumbing | `chore/audit-rustsec-bumps` |
| `docs/<slug>` | Documentation-only changes | `docs/onboarding-macos` |
| `spec/<slug>` | Spec Kit cycle artifacts only | `spec/s5-close-executor` |
| `dependabot/...` | Dependabot-managed (don't rename) | `dependabot/cargo/...` |

**Rules:**

- Branch name MUST start with one of the prefixes above (or `dependabot/` for bot PRs).
- Slugs use lowercase + hyphens; no underscores, no slashes (other than the prefix separator).
- For Spec-Kit-driven work tied to a P-NN prompt, the slug starts with the prompt code: `feat/p05-mapper`, `feat/p10-credential-broker`.
- Do not rebrand existing branches; convention applies going forward.
- The pre-push hook does NOT enforce branch names today (manual review). If naming drift accumulates we'll add a hook.

## Release tags

Tags are `vMAJOR.MINOR.PATCH` (semver). The release pipeline at
`.github/workflows/release.yml` triggers on `v*` tag pushes, builds
Linux + macOS binaries, and publishes a GitHub Release.

| Increment | When to bump |
|---|---|
| MAJOR | Breaking CLI/profile/config changes |
| MINOR | New features (e.g., new cloud provider, new agent) |
| PATCH | Bug fixes + install / docs / dependency hygiene |

Stage gates from `docs/governance/SESSION_PLAN.md` map to MINOR
versions: Stage 1 MVP → 0.1.x, Stage 2 agentic → 0.2.x, Stage 3
multi-cloud → 0.3.x, etc.

## Commit message conventions

Conventional Commits with these scopes:

- `feat(<scope>)` — new feature
- `fix(<scope>)` — bug fix
- `refactor(<scope>)` — pure refactor (no behaviour change)
- `chore(deps)` / `chore(ci)` — dependency / CI hygiene
- `docs(...)` — documentation only

Co-author with Claude when applicable:

```
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
```

## Adding a new agent

The project permits exactly three agents — **Recovery**, **Cost Optimizer**,
and **Cutover**. Adding any other agent requires an **RFC + stage-gate
approval**. Open a feature-request issue with `kind = "New agent"` and link
the RFC discussion. We mean it.

Stage 1 ships with zero agents. Stage 2 introduces Recovery and Cost Optimizer.

## License

By contributing, you agree your contribution is licensed under
[Apache 2.0](LICENSE).
