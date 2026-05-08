# Tasks — s5-close-executor

Atomic, dependency-ordered. Each task ≤30 LOC.

| # | Task | File | Depends |
|---|---|---|---|
| **PHASE A — Profile + Cred broker** | | | |
| T1 | Add `CredConfig` struct + `CredMode` enum to profile module | `libs/shared/src/profile.rs` | — |
| T2 | Profile parser test: `[profiles.default.creds.aws]` round-trips with all CredMode variants | `libs/shared/src/profile.rs` | T1 |
| T3 | Add `aws-sdk-sts` to workspace `Cargo.toml` (feature `behavior-version-latest`) | `Cargo.toml` | — |
| T4 | Implement `aws::Broker::resolve_sts_assume_role(cfg) -> Result<Credential, CredsError>` | `libs/creds/src/aws.rs` | T1, T3 |
| T5 | AWS broker tests: stub STS client returns canned credentials; assert resolution_method = "sts_assume_role"; expires_at within bounds | `libs/creds/src/aws.rs` | T4 |
| T6 | Add `google_cloud_auth` to workspace deps | `Cargo.toml` | — |
| T7 | Implement `gcp::Broker::resolve_adc()` delegating to `google_cloud_auth::default()` | `libs/creds/src/gcp.rs` | T1, T6 |
| T8 | GCP broker tests: env-var ADC happy path (`GOOGLE_APPLICATION_CREDENTIALS=...`) + clean error when absent | `libs/creds/src/gcp.rs` | T7 |
| T9 | Add `azure_identity` to workspace deps | `Cargo.toml` | — |
| T10 | Implement `azure::Broker::resolve_service_principal_env()` reading `ARM_*` env vars | `libs/creds/src/azure.rs` | T1, T9 |
| T11 | Implement `azure::Broker::resolve_managed_identity()` calling IMDS via `azure_identity::ManagedIdentityCredential` | `libs/creds/src/azure.rs` | T10 |
| T12 | Azure broker tests: SP env happy path + MI mock + absent → clean error | `libs/creds/src/azure.rs` | T11 |
| T13 | Add `CredentialBroker::resolve_for_cloud(cloud, profile)` dispatcher | `libs/creds/src/broker.rs` | T4, T7, T10 |
| T14 | Article XIII rule 7 invariant test: grep `libs/creds/src/` for `std::fs::write` patterns; assert none touch `Credential::*` | `libs/creds/tests/no_disk_secret_test.rs` | T13 |
| **PHASE B — Executor subprocess + Docker isolation** | | | |
| T15 | Add `SubprocessRunner` trait to `libs/engine/src/executor/runner.rs` (NEW file) | `libs/engine/src/executor/runner.rs` | — |
| T16 | Implement `LocalRunner` using `tokio::process::Command` with env injection + line-streaming tee | `libs/engine/src/executor/runner.rs` | T15 |
| T17 | LocalRunner tests: spawn `echo hello`, assert stdout captured + log file written | `libs/engine/src/executor/runner.rs` | T16 |
| T18 | Implement `DockerRunner` building `docker run --rm -v <cwd>:/work -w /work -e K=V... hashicorp/terraform:1.10 <args>` | `libs/engine/src/executor/runner.rs` | T15 |
| T19 | DockerRunner builder tests: arg shape matches reference (no actual docker spawn) | `libs/engine/src/executor/runner.rs` | T18 |
| T20 | New `audit_emit.rs` helper: `emit_tool_execution(audit, tool, args, exit, duration)` → appends to chain | `libs/engine/src/executor/audit_emit.rs` | — |
| T21 | Wire `runner.run()` + audit emission into `Executor::apply()`, replacing `NotImplementedYet` | `libs/engine/src/executor/mod.rs` | T16, T18, T20 |
| T22 | Backup-first wrapper: before any subprocess, save `<cwd>/.terraform/` + `<cwd>/terraform.tfstate` to `<cwd>/.terrashift/runs/<run_id>/backups/` | `libs/engine/src/executor/mod.rs` | T21 |
| T23 | Post-execution scrub: pipe log file through `creds::scrub::scrubber()`; abort + audit `SecretLeak` on match | `libs/engine/src/executor/mod.rs` | T21 |
| T24 | Integration test: stub `SubprocessRunner` records args + returns canned exit codes; assert audit chain has 4 ToolExecution events for the canonical `init/validate/plan/apply` sequence | `libs/engine/tests/apply_e2e_test.rs` | T21 |
| **PHASE C — CLI + smoke** | | | |
| T25 | New `cli/src/commands/apply.rs` with `ApplyArgs` struct + `run(args)` skeleton | `cli/src/commands/apply.rs` | — |
| T26 | Auto-detect cloud from HCL: walk `<path>/*.tf`, parse provider blocks, return single cloud or `MultipleProviders` error | `cli/src/commands/apply.rs` | T25 |
| T27 | Wire profile load → broker dispatch → cred resolution → executor invocation | `cli/src/commands/apply.rs` | T13, T21, T26 |
| T28 | `--approve` flag: swap to `stage1_policy_with_explicit_apply()` policy variant | `cli/src/commands/apply.rs` + `libs/shell-tool-approvals/src/lib.rs` | T27 |
| T29 | `--no-sandbox` flag: select `LocalRunner` instead of `DockerRunner` | `cli/src/commands/apply.rs` | T16, T18, T27 |
| T30 | Migration summary print: commands run, exit codes, audit chain head hash, total wall time | `cli/src/commands/apply.rs` | T27 |
| T31 | Add `apply` to clap subcommand dispatcher in `cli/src/main.rs` | `cli/src/main.rs` | T30 |
| T32 | CLI smoke test: `terrashift apply <fixture> --no-sandbox --cloud aws` with stub broker + stub runner; assert exit 0 + summary lines | `cli/tests/apply_smoke_test.rs` | T31 |
| **PHASE D — Polish + ship** | | | |
| T33 | `cargo fmt --all` clean | (workspace) | T1-T32 |
| T34 | `cargo clippy --workspace --all-targets -- -D warnings` clean | (workspace) | T33 |
| T35 | `cargo test --workspace` clean | (workspace) | T34 |
| T36 | Manual smoke: build binary, `terrashift apply ./fixtures/e2e-aws-to-azure-azurerm/ --no-sandbox --approve` (operator's machine, with creds) | (manual) | T35 |
| T37 | Update `SESSION_PLAN.md` ledger: S5 row gets "✅ Done — Executor + Cred broker shipped (this PR)" | `docs/governance/SESSION_PLAN.md` | T35 |
| T38 | `/speckit-git-commit` + push branch + open PR with full Constitution + stakpak_arch.md citation block | (git) | T37 |

## Critical path

T1 → T4 → T13 → T21 → T27 → T30 → T31 → T35 → T38

That's the minimum viable shipping path: profile schema → AWS cred backend → broker dispatch → executor wiring → CLI command → all tests pass → ship.

The other tasks (GCP/Azure backends, Docker runner, secret scrub, etc.) are required for the full S5 close but not for the first incremental commit. Recommend committing after T14 (broker complete), T24 (executor complete), and T35 (CLI complete) so the PR has 3 logical commits.

## Estimated work

- Phase A: 1.5h (4 broker backends + tests)
- Phase B: 1.5h (subprocess runner + audit + tests)
- Phase C: 1h (CLI surface + smoke)
- Phase D: 0.5h (polish + ship)

**Total: ~4.5h** focused work. Realistic upper bound 6h with debugging.
