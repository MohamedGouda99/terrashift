# Terrashift Constitution

> **Mirror of `CONSTITUTION.md` at the workspace root.**
> The workspace-root file is the human-readable canonical version. This file
> is the executable mirror used by Spec Kit commands (`/speckit-constitution`,
> `/speckit-clarify`, `/speckit-analyze`).
> When they diverge, the root file wins. Re-port via this file's
> "Synchronisation" section below.

## Core Principles

### I. Architectural Restraint (NON-NEGOTIABLE)

Of the nine pipeline components, only three are agents (Recovery, Cost
Optimizer, Cutover). The other six (Scanner, Mapper, Planner, Generator,
Validator, Executor, Verifier — Mapper and Planner are LLM-with-structured-
output, the rest are pure deterministic Rust) do not loop.

Adding a new agent requires explicit RFC and stage-gate approval.

Sub-clause I.4: agents have bounded blast radius via the subagent permission
model from `stakpak_arch.md` section 8. Each agent has a scoped tool set
registered at construction time; permission scoping is enforced at the
registry boundary, not at call time.

### II. Reference Codebase Discipline

Stakpak is the primary architectural reference, with `stakpak_arch.md`
(~2,840 lines) as the canonical document. Claude Code is the secondary
reference for agent-loop concepts. All three are checked out locally on every
team member's machine at `~/refs/`.

PR descriptions cite which `stakpak_arch.md` sections informed each
implementation choice. Code comments include a `// Pattern: stakpak_arch.md
section N` line at the top of any file that adopted a Stakpak pattern.

### III. AI Safety (NON-NEGOTIABLE)

Every LLM-emitted attribute is checked against the live provider schema
before HCL emit. Hallucinations are a build break, not a runtime warning.
The Validator (`libs/engine/src/validator`) is the single enforcement point
and **must** be on the path between Mapper and Generator. Bypassing the
Validator is a CI failure.

### IV. Failure Mode Handling (NON-NEGOTIABLE)

Failures are loud. No silent fallbacks, no swallowed errors. If you can't
tell the user what went wrong, don't continue. Operationalised via Article
XIII rule 3 (no `unwrap`/`expect`/`&s[..n]` in production code paths) and
the workspace-level Clippy lint posture from `stakpak_arch.md` section 31.

### V. Credentials and Security (NON-NEGOTIABLE)

No long-lived credentials in process memory. **Two credential classes** are
protected:

1. **Cloud provider credentials** — AWS STS, GCP ADC + workload identity
   federation, Azure managed identity. Short-lived federated tokens only,
   never long-lived keys.
2. **LLM provider keys** — Anthropic, OpenAI, custom Vodafone gateway, etc.
   `api_key_env` references resolved by the broker at runtime, zeroized
   after each request, never held as instance fields on the `Client` struct.

The LLM never sees raw secrets of either class — only references resolved at
the tool-execution boundary, mirroring the secret substitution pattern in
`stakpak_arch.md` section 27. Every credential operation is audited.

Audit entries for LLM-touching operations record `provider`, `model_id`, and
`provider_endpoint`. Customer-supplied API keys never appear in any log,
trace span, or error message — the redaction layer (Article XIII rule 5)
catches them at the proxy boundary.

Article XIII rules 5, 6, 7, and 10 are the concrete enforcement of
Principle V's principles.

### VI. Knowledge Layer Integrity

Provider schemas are version-pinned per migration. The same migration today
produces the same output six months from now. No "latest" anywhere in
production paths. Per `stakpak_arch.md` section 23, version pinning is what
makes prompt caching work — non-monotonic cache behaviour kills cache hits
and breaks Principle XII rule 2.

### VII. Repository Hygiene

Trunk-based development. No long-lived feature branches. PRs ≤500 lines
preferred. Constitution principles cited in PR descriptions.

CI enforces the lint posture from `stakpak_arch.md` section 31:

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
string_slice = "deny"
```

### VIII. Stage Gates

Stages don't progress without exit-gate sign-off. Stage 1 ships before Stage 2
starts. No parallel stage development.

**Sub-clause VIII.3:** "adding agentic complexity in Stage 2 must not
increase happy-path Stage-1 migration cost by more than 1.3×." The same
principle applies to every stage transition.

### IX. Data Governance

State files, audit logs, and migration outputs are user property. We never
delete user data automatically. Archival, never deletion.

Audit log entries written via `libs/audit` are append-only and chain-verified.

### X. Observability

Every LLM call, every tool invocation, every pipeline stage gets a `tracing`
span. Logs are structured. No `println!` in production code paths. Mirrors
`stakpak_arch.md` section 32's tracing pattern with optional OpenTelemetry
export behind a feature flag.

LLM call spans include attributes: `provider`, `model_id`, `tier`,
`input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_write_tokens`,
`cost_usd_micros`. Tool execution spans include `tool_name` and `duration_ms`.
These are non-optional — Principle V's audit invariant requires them.

### XI. Amendments

The constitution is amended by RFC. Amendments are versioned. Major version
bumps require team consensus. Article XIII can grow over time as new
anti-patterns are discovered during development; growth itself is a
Principle XI amendment, recorded in this file's git history.

### XII. Token Economy

Five enforceable rules:

1. Every component has a documented per-tier cost ceiling against the eval
   baseline. The ceiling is per-tier (eco / smart) not per-migration; absolute
   per-migration costs depend on which model the customer chose within each
   tier (BYOK choice).
2. Cache-first: lookup → vector search → cold LLM call, in that order. Per
   Article XIII rule 2, cache-first behaviour requires monotonic trim
   boundaries.
3. Tiered routing: every LLM-using component is bound to a tier (eco /
   smart), not a specific model.
4. CI regression gate: >30% token cost increase against the prior baseline
   blocks PR merge. Protects customers from silent system-wide cost drift
   even when their model choice changes.
5. Monthly review: top-10 most expensive prompts (regardless of tier) are
   audited and optimized.

Principle XII is enforced by the eval framework's token-tracking
(`libs/eval`), not by reviewer vigilance.

## Source-Derived Anti-Patterns (Article XIII)

Ten anti-patterns lifted directly from `stakpak_arch.md` section 42 —
battle-scarred lessons from 295 branches and ~177k LOC of Rust development on
Stakpak.

| # | Anti-pattern | Maps to |
|---|---|---|
| 1 | Don't bypass the message conversion pipeline. Skipping `ContextReducer::reduce` and feeding raw messages to the LLM produces Anthropic 400s on dangling `tool_use` blocks. | Principle III |
| 2 | Don't make trim boundaries non-monotonic. If `trimmed_up_to_message_index` ever goes backward, every Anthropic prompt-cache hit dies. | Principle XII rule 2 |
| 3 | Don't `unwrap()` / `expect()` / `&s[..n]` outside tests. Clippy must block these in production crates. | Principle IV |
| 4 | Don't conflate `ChatMessage` and `LLMMessage`. Storage type vs runtime type. Mixing them creates conversion mistakes that compile but fail at runtime. | Type discipline; enforced via `libs/shared` boundary |
| 5 | Don't bypass the proxy redaction layer. If a tool result reaches the LLM without going through `redact_content`, secrets leak. | Principle V |
| 6 | Don't approve `run_command` (or `terraform apply`) wholesale. Use tree-sitter command-level approval. | Principle V |
| 7 | Don't write disk-bound secrets to make sandbox UID issues "go away". | Principle V |
| 8 | Don't add a new `OutputEvent`/`InputEvent` variant without updating `is_backend_event()`. Silent UI freezes. | Principle IV |
| 9 | Don't push a `tool_result` for a tool that was Cancelled when the queue is empty. Duplicate `tool_call_id` breaks Anthropic. | Operational invariant |
| 10 | Don't store an API key in a profile that lives outside `~/.terrashift/`. Credential resolution chain depends on file location. | Principle V |

## Development Workflow

### PR description template

PR descriptions must include a "Constitution" section listing the principles
touched:

```markdown
## Constitution
- Principle III (Validator gate enforced — see new test in libs/engine/tests/validator_hallucination.rs)
- Principle V (no new credential surface; existing broker handles new provider key)
- Article XIII rule 5 (redaction boundary unchanged; verified by integration test)

## stakpak_arch.md References
- section 8 (agent-core kernel)
- section 27 (secret detection / redaction)

## Eval Impact
- Token cost delta: -2.3% vs baseline
- Cache hit rate: 96% (within target)
```

Reviewers check the cited principles match the actual change shape.
Mismatches are a request-changes signal.

### Spec Kit feature workflow

```
/speckit-git-feature → /speckit-specify → /speckit-clarify
  → /speckit-plan → /speckit-tasks → /speckit-implement
  → /speckit-analyze → /speckit-checklist → /speckit-git-commit
```

Never skip `/speckit-clarify`. It's where v5's ambiguities surface — resolving
them before code is 10× cheaper than after.

## Governance

This file (`.specify/memory/constitution.md`) is the executable mirror of
`CONSTITUTION.md` at the workspace root. The root file is the canonical,
human-readable version. When they diverge, **the root file wins** — re-port
this file from the root.

All PRs/reviews must verify compliance. Complexity must be justified.
Use `CLAUDE.md` for runtime development guidance and `pre-flight.md` for
environment-specific decisions.

### Synchronisation

When `CONSTITUTION.md` is amended (Principle XI), update this file:

1. Open `CONSTITUTION.md` and identify the changed Article(s).
2. Update the matching Principle in this file.
3. Update Article XIII table if a new anti-pattern was added.
4. Bump the version below.
5. Commit both files in a single PR with `chore(constitution): sync to vN.M.K`.

**Version**: 1.0.0 | **Ratified**: 2026-05-02 | **Last Amended**: 2026-05-02
