# Feature Specification: S5 close — Executor + Cred broker for real `terraform apply`

**Feature Branch**: `feat/s5-close-executor`
**Created**: 2026-05-08
**Status**: Draft
**Stage:** 1 (closing) → 2 (Recovery during apply lands later)
**P-NN prompts covered**: P-09 (Executor) close + P-10 (Cred broker) close

---

## Background

Stage 1 shipped the **shells** for both Executor and Cred broker. Today
both return `NotImplementedYet` or `StubBroker` placeholder values:

- `libs/engine/src/executor/mod.rs::Executor::apply()` — gate logic,
  audit emission seams, run-dir scaffolding all built; subprocess
  invocation returns `ExecutorError::NotImplementedYet { session: "S5" }`.
- `libs/creds/src/stub.rs` — `StubBroker` returns hard-coded
  placeholder strings; real `aws.rs` / `gcp.rs` / `azure.rs` modules
  exist as scaffolds but are unwired.
- No CLI command — `terrashift apply <path>` does not exist.

**This spec closes the loop.** When done, an operator can run:

```
terrashift migrate --source aws-app --from aws --to azurerm
terrashift apply ./aws-app-azurerm/    # ← new
```

The second command runs `terraform plan` + `terraform apply` against
the migrated HCL with short-lived BYOK creds, all gated by the
P-09a approval policy, fully audited.

## Constitution Articles in Scope

- **Article V** — heart of it. Sandboxed apply, command-level approval, in-memory secrets only.
- **Article X** — every action emits an audit event with a tracing span.
- **Article XII rule 4** — apply runs add no LLM tokens (Stage 1 close: pure deterministic).
- **Article XIII rule 6** — approval gate is non-optional in the apply path.
- **Article XIII rule 7** — no disk-bound secret writes; env vars only.

## stakpak_arch.md References

- §15 (executor pattern, terraform binary subprocess)
- §16 (cred broker as single secret-source-of-truth)
- §27 (secret detection / redaction at tool-execution boundary)
- §29 (Warden sandbox — Docker isolation as Stage 1 default)
- §30 (shell command-level approvals via tree-sitter-bash)

## User Scenarios

### US1 — Apply migrated HCL with AWS BYOK creds (P1)

Operator has migrated AWS→Azure HCL. They want to deploy it to Azure with their own
short-lived Azure credentials.

**Acceptance**:
1. Given `terrashift migrate ...` produced `./aws-app-azurerm/main.tf` etc.,
2. When operator runs `terrashift apply ./aws-app-azurerm/ --cloud azurerm`,
3. Then Executor runs `terraform init`, `terraform validate`, `terraform plan -out plan.tfplan`,
4. And Cred broker resolves Azure creds (managed identity, env-var ADC fallback, or service principal env vars).
5. And every command emits an `AuditPayload::ToolExecution` event with prev_hash chained.
6. And `terraform apply plan.tfplan` is **denied by default** in non-interactive mode (Stage 1: explicit `--approve` flag required to bypass; full TUI prompt deferred to Stage 5).

### US2 — Cred resolution from operator profile (P1)

Operator's `~/.terrashift/profile.toml` declares cloud cred sources:

```toml
[profiles.default.creds.aws]
mode = "sts_assume_role"
role_arn = "arn:aws:iam::123:role/terrashift-deploy"
session_name = "terrashift"

[profiles.default.creds.azurerm]
mode = "service_principal_env"   # reads ARM_CLIENT_ID etc. from env

[profiles.default.creds.google]
mode = "adc"                     # gcloud application-default-credentials
```

**Acceptance**:
1. Given the profile above and the operator running `terrashift apply ./out --cloud aws`,
2. When Executor invokes `terraform plan`,
3. Then before the subprocess starts, Cred broker calls AWS STS `AssumeRole`,
4. And the resulting STS short-lived token is set as `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN` env vars on the subprocess only,
5. And the parent process `env` does NOT contain those vars (subprocess inherits via `Command::env()`).
6. And the subprocess output to stdout/stderr is scanned for secret leakage; any match raises `AuditPayload::SecretLeak` and aborts the apply.

### US3 — Sandboxed execution via Docker (P2)

By default, terraform runs inside a Docker container so a malicious
provider plugin can't read the host filesystem.

**Acceptance**:
1. Given `terrashift apply ./out --cloud aws` with the default sandbox setting,
2. When Executor builds the subprocess command,
3. Then it spawns `docker run --rm -v ./out:/work -w /work -e AWS_ACCESS_KEY_ID=... hashicorp/terraform:1.10 plan`,
4. And the host filesystem outside `./out` is not bind-mounted into the container.

**Out of scope for Stage 1 close**: Docker daemon detection / fallback to
local `terraform` binary when Docker unavailable. That's a follow-up
(`--no-sandbox` flag or ergonomic auto-fallback in S15).

### US4 — Apply outcome reported and audited (P1)

**Acceptance**:
1. After every `terrashift apply` run (success or failure),
2. The CLI prints a summary: commands authorized, subprocess exit codes, total wall time, audit chain head hash.
3. The audit DB has one `AuditPayload::ToolExecution` row per command + one `AuditPayload::PhaseTransition` row marking apply-start and apply-end.
4. The audit chain integrity is verifiable by `terrashift audit verify <run_id>`.

### Edge Cases

- **Subprocess timeout**: `terraform apply` for a 100-resource plan can take 30+ minutes. Default timeout: 60min, configurable per-command.
- **Subprocess killed mid-apply**: partial state in `terraform.tfstate` is recoverable. The apply emits `PhaseTransition::Aborted` and the operator can re-run.
- **Cred broker fails (e.g., STS AssumeRole denied)**: Executor refuses to invoke any subprocess; emits a clean error with the underlying STS message.
- **Env-var ADC for GCP not set**: clean error pointing to `gcloud auth application-default login`.
- **`terraform.tfstate` already exists in target dir**: backed up to `.terrashift/runs/<run_id>/backups/` per Article V reversibility.

## Requirements

### Functional

- **FR-1**: `terrashift apply <dir>` is a new top-level CLI subcommand.
- **FR-2**: Executor's `apply()` method, when given an `Allow` verdict, spawns a real subprocess (Docker or local terraform binary).
- **FR-3**: Cred broker's `aws::Broker` implements STS `AssumeRole` end-to-end; `gcp::Broker` reads ADC; `azure::Broker` reads env-var service principal OR managed identity.
- **FR-4**: Subprocess env vars are set via `Command::env()` only (Article XIII rule 7); never written to disk.
- **FR-5**: Subprocess stdout/stderr is scanned for secret leakage post-execution; matches abort the apply.
- **FR-6**: Every authorized command emits `AuditPayload::ToolExecution` with `tool_name = "terraform"`, `args`, `exit_code`, `duration_ms`.
- **FR-7**: `terraform apply` is denied by default in non-interactive mode; `--approve` flag bypasses the deny verdict.
- **FR-8**: Profile-driven cred mode selection per cloud (`mode = "sts_assume_role" | "adc" | "service_principal_env" | "managed_identity"`).

### Non-Functional

- **NFR-1**: Apply run with empty plan completes <30s on a clean Docker daemon.
- **NFR-2**: Subprocess output is streamed to stdout (not buffered); operator sees terraform progress in real-time.
- **NFR-3**: Memory for cred values is zeroed on drop (already enforced via `Zeroizing<String>` in `Credential`).
- **NFR-4**: No new workspace dependencies for happy path; STS/ADC client crates are gated behind feature flags so single-binary install size doesn't bloat.

## Success Criteria

- **SC-1**: An end-to-end test (`tests/apply_e2e_test.rs`) takes a fixture HCL dir, runs `terrashift apply` against a stub Docker that just echoes args, asserts the audit chain has the expected events.
- **SC-2**: Profile-driven cred mode test: `tests/cred_resolution_test.rs` covers the 4 modes with mock STS/ADC/MI responses.
- **SC-3**: Article XIII rule 7 invariant test: no env var containing `AWS_SECRET_ACCESS_KEY`-like patterns leaks into the parent process after apply returns.
- **SC-4**: `cargo test --workspace` passes; clippy clean; fmt clean.
- **SC-5**: Manual run against `fixtures/e2e-aws-to-azure-azurerm/` (output of canonical migrate) with real Azure creds — `terraform plan` succeeds.

## Article XIII Rules in Play

- **Rule 1** (no bypass of message conversion pipeline) — N/A; Executor is post-LLM.
- **Rule 5** (no path literals in tests) — preserved; `tempfile::tempdir()` for run dirs.
- **Rule 6** (gate non-optional) — `apply()` calls `pre_apply_check` for every command; CI test will assert no path bypasses it.
- **Rule 7** (no disk-bound secrets) — `Command::env()` only; CI grep test fails on `std::fs::write.*credential`.

## Out of Scope (deferred)

- **Recovery during apply** — Recovery agent re-prompts on `terraform validate` / `apply` failures. S10 territory; this PR ships the apply path; Recovery wiring on it is a follow-up session (S5+S10 integration).
- **Cost preview before apply** — Infracost integration. S11 territory.
- **Detached mode** — apply runs in background, operator polls for completion. S13.
- **Drift detection** — `terraform plan` after apply to confirm no drift. S33.
- **Real Docker daemon detection + auto-fallback to local terraform** — Stage 2 polish; this spec assumes Docker is available.
