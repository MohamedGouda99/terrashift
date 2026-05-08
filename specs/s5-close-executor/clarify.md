# Clarifications — s5-close-executor

These are the genuine ambiguities surfaced by spec.md. Each is resolved
explicitly so the implementation doesn't drift back into ambiguity.

## Q1: CLI invocation shape

**Surface**: `terrashift apply <where?> <how?>`.

**Options**:
- (A) `terrashift apply <path>` (positional)
- (B) `terrashift apply --target <path>` (named flag, symmetric with `migrate --source`)
- (C) `terrashift apply` (assume current dir)

**Decision**: **(A)** with auto-detect cloud from `provider "X" {}` blocks
in the HCL. `--cloud <name>` flag overrides when HCL has multiple
providers or none. This matches `terraform apply` ergonomics —
operators already know "give me a path, run apply on it."

## Q2: Default sandbox mode

**Surface**: Should Stage 1 close run terraform in Docker by default, or
fall back to local binary?

**Options**:
- (A) Docker default; `--no-sandbox` to opt out
- (B) Local binary default; `--sandbox` to opt in
- (C) Auto-detect Docker daemon; fall back silently

**Decision**: **(A)**. Article V (sandboxed apply) treats Docker isolation
as the default posture. Silent fallback (C) violates Article IV
(loud failures) — operator must know whether they're in sandbox mode.
`--no-sandbox` is an explicit operator opt-in.

If Docker isn't installed, error: `Docker required for sandboxed apply
— install Docker or pass --no-sandbox to use local terraform binary
(see docs/security/no-sandbox-tradeoffs.md)`.

## Q3: `--approve` flag semantics

**Surface**: How does `--approve` interact with the P-09a approval gate?

**Options**:
- (A) `--approve` skips the gate entirely (back-door)
- (B) `--approve` swaps to a permissive policy where `terraform apply` is `Allow`
- (C) `--approve` keeps gate; requires per-command operator confirmation in TUI

**Decision**: **(B)**. Article XIII rule 6 says the gate is non-optional.
A back-door (A) violates it. Per-command TUI prompts (C) are a Stage 5
goal but not buildable without the agent loop kernel's prompt path.
Swapping to a `stage1_policy_with_explicit_apply()` variant keeps the
gate logic intact and just flips one rule.

## Q4: STS AssumeRole — role_arn source

**Surface**: Where does the role ARN come from for `mode = "sts_assume_role"`?

**Options**:
- (A) Profile only
- (B) CLI flag override
- (C) Env var override

**Decision**: **(A) for Stage 1 close**. Profile lives at
`~/.terrashift/profile.toml`; cred sources are operator-configured
infrastructure, not per-invocation. CLI flag override is a follow-up
(`--role-arn arn:...`) when we have a use case for it.

## Q5: GCP ADC fallback chain

**Surface**: GCP's ADC has its own fallback chain (env → metadata → gcloud config). Replicate or delegate?

**Decision**: **Delegate to the SDK**. Stage 1 close uses
`google_cloud_auth::default()` — it walks the fallback chain. Our
broker just translates "any failure" into a clean
`CredsError::GcpAdcUnavailable { detail }`.

## Q6: Audit on subprocess output streaming

**Surface**: Subprocess output goes to operator's stdout (NFR-2). Do we also capture it?

**Decision**: **Both**. Stream to operator's stdout for visibility; tee
to `.terrashift/runs/<run_id>/logs/<n>-<command>.log`. The log file is
post-scanned for secret leakage; matches abort with
`AuditPayload::SecretLeak`.

## Q7: Re-run safety

**Surface**: `terrashift apply ./out` run twice. What happens?

**Decision**: **Always run `terraform plan` first.** If plan is empty
(`exit 0`, "No changes"), skip apply with a clean
`✓ No changes — apply skipped`. No special re-run idempotency layer
on top of terraform's own state machine.

## Q8: Secret leak detection — reuse existing scrubber

**Decision**: **Reuse `libs/creds/src/scrub.rs`'s scrubber**. Hook it to
subprocess stdout/stderr post-execution. No new regex set; if the
scrubber is wrong, fix it once in scrub.rs (single source of truth).

## Q9: Working directory

**Surface**: Where does the subprocess run? `<path-arg>` directly, or a
copy in `/tmp/terrashift-exec-<run_id>/`?

**Decision**: **Run in the path-arg directory directly.** `terraform`
writes `.terraform/`, `terraform.tfstate`, `plan.tfplan` there — that's
where the operator wants them. Backup-first wrapper (Article V) saves
existing `.terraform/` and `.tfstate` to
`.terrashift/runs/<run_id>/backups/` before subprocess starts.

## Q10: Apply on a directory with no `provider {}` block

**Decision**: **Loud error** (Article IV). `terrashift apply ./out`
where `./out/*.tf` has zero provider blocks → error pointing to which
files were checked + suggesting `--cloud <name>` flag override.

---

## Summary of decisions

| Q | Resolution |
|---|---|
| Q1 | `terrashift apply <path>` positional + `--cloud` override |
| Q2 | Docker default; `--no-sandbox` to opt out; loud error if Docker unavailable |
| Q3 | `--approve` swaps to permissive policy variant; gate stays on path |
| Q4 | Profile-only role_arn for Stage 1 close |
| Q5 | Delegate to GCP SDK's default ADC resolver |
| Q6 | Stream to operator stdout + tee to per-command log file |
| Q7 | Always plan first; skip apply when plan is empty |
| Q8 | Reuse `creds::scrub` scrubber for subprocess output |
| Q9 | Subprocess runs in path-arg dir; backup-first wrapper for `.terraform/`+`.tfstate` |
| Q10 | Loud error when no provider block found |
