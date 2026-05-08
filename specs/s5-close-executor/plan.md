# Implementation Plan — s5-close-executor

**Spec**: `specs/s5-close-executor/spec.md`
**Clarify**: `specs/s5-close-executor/clarify.md`
**Branch**: `feat/s5-close-executor` (off `origin/main`, NOT off the r07 stack)
**Constitution articles in scope**: V, X, XII rule 4, XIII rules 6, 7
**stakpak_arch.md sections referenced**: §15 (executor), §16 (broker), §27 (redaction), §29 (Warden), §30 (shell approval)

## Architectural shape

Three orthogonal sub-features ship together:

1. **Executor real subprocess** (`libs/engine/src/executor/`)
   - Replace `NotImplementedYet` with real `tokio::process::Command`.
   - Docker-isolation builder: `docker run --rm -v <path>:/work -w /work -e KEY=VAL... hashicorp/terraform:1.10 <args>`.
   - Local fallback when `--no-sandbox` set: `terraform <args>` directly.
   - Stream stdout/stderr line-by-line to operator + tee to per-command log file.
   - Each authorized command emits `AuditPayload::ToolExecution`.

2. **Cred broker real backends** (`libs/creds/src/{aws,gcp,azure}.rs`)
   - `aws::Broker::resolve()` calls AWS STS AssumeRole via `aws-sdk-sts`.
   - `gcp::Broker::resolve()` delegates to `google_cloud_auth::default()`.
   - `azure::Broker::resolve()` reads service principal env vars OR managed identity (`azure_identity::DefaultAzureCredential`).
   - All three emit `Credential` (Zeroizing-wrapped, with resolution_method + expires_at).
   - Profile config drives mode selection.

3. **`terrashift apply` CLI** (`cli/src/commands/apply.rs`)
   - New top-level command: `terrashift apply <path> [--cloud X] [--no-sandbox] [--approve] [--profile P]`.
   - Auto-detects cloud from `provider "X" {}` blocks in HCL.
   - Wires Cred broker → Executor → Audit.

## File changes

### `libs/engine/src/executor/mod.rs` ([MODIFY], +250)

1. Replace `NotImplementedYet` paths with real subprocess invocation.
2. New `SubprocessRunner` trait: `async fn run(&self, command: &[&str], env: &[(String, String)], cwd: &Path) -> Result<SubprocessOutcome, ExecutorError>`.
3. Two impls: `DockerRunner` (default), `LocalRunner` (--no-sandbox).
4. `apply()` walks commands, calls `pre_apply_check`, then `runner.run()` with cred-broker-resolved env vars.
5. Stream tee: `tokio::io::BufReader::lines()` → tokio::select! between stdout + stderr → write to operator + log file.
6. Post-execution scrub: read log file, run scrubber, abort + emit `SecretLeak` audit on match.

### `libs/engine/src/executor/runner.rs` ([NEW], +120)

`SubprocessRunner` trait + Docker/Local impls. Separated for testability.

### `libs/engine/src/executor/audit_emit.rs` ([NEW], +40)

Helper that wraps a `SubprocessOutcome` into an `AuditPayload::ToolExecution` event and appends to the chain.

### `libs/creds/src/aws.rs` ([MODIFY], +80)

Real STS AssumeRole. Profile config:
```toml
[profiles.default.creds.aws]
mode = "sts_assume_role"  # or "static_env" for legacy
role_arn = "arn:aws:iam::123:role/terrashift-deploy"
session_name = "terrashift"
duration_seconds = 3600  # default 1h, max 12h
external_id = "..."  # optional
```

Uses `aws-sdk-sts` (already a workspace dep — verify; if not, add).

### `libs/creds/src/gcp.rs` ([MODIFY], +50)

Delegate to `google_cloud_auth` crate's `default()` resolver. Wrap result.

### `libs/creds/src/azure.rs` ([MODIFY], +60)

Two modes: `service_principal_env` (read env, return as-is) and `managed_identity` (call IMDS endpoint via `azure_identity::ManagedIdentityCredential`).

### `libs/creds/src/broker.rs` ([MODIFY], +30)

`CredentialBroker` trait gains a `resolve_for_cloud(cloud: &str, profile: &Profile) -> Result<Credential, CredsError>` dispatcher that picks the right backend.

### `cli/src/commands/apply.rs` ([NEW], +180)

```rust
pub struct ApplyArgs {
    pub path: PathBuf,
    pub cloud: Option<String>,
    pub no_sandbox: bool,
    pub approve: bool,
    pub profile: Option<PathBuf>,
}

pub async fn run(args: ApplyArgs) -> Result<()> {
    // 1. Load profile
    // 2. Detect cloud (auto from HCL provider blocks, or args.cloud)
    // 3. Resolve cred via broker
    // 4. Build Executor (sandbox=true unless --no-sandbox; policy=permissive_apply if --approve)
    // 5. Run Executor::apply with the canonical command sequence:
    //    [terraform init, terraform validate, terraform plan -out plan.tfplan, terraform apply plan.tfplan]
    // 6. Print summary; flush audit chain
}
```

### `cli/src/main.rs` ([MODIFY], +5)

Add `apply` subcommand to the clap dispatcher.

### `libs/shared/src/profile.rs` (or wherever Profile is defined) ([MODIFY], +25)

Add `[profiles.X.creds.{aws,gcp,azure}]` schema:
```rust
pub struct CredConfig {
    pub mode: CredMode,
    pub role_arn: Option<String>,
    pub session_name: Option<String>,
    pub duration_seconds: Option<u32>,
    pub external_id: Option<String>,
}

pub enum CredMode {
    StsAssumeRole,
    StaticEnv,
    Adc,
    ServicePrincipalEnv,
    ManagedIdentity,
}
```

### Tests

#### Unit
- `libs/engine/src/executor/runner.rs` — `LocalRunner` happy path with stub `echo` command; `DockerRunner` builder verifies arg shape (don't actually spawn docker).
- `libs/creds/src/aws.rs` — STS AssumeRole with mock STS endpoint via `aws-sdk-sts` test client.
- `libs/creds/src/gcp.rs` — env-var ADC happy path; missing ADC clean error.
- `libs/creds/src/azure.rs` — env-var SP happy path; absent → clean error.

#### Integration
- `libs/engine/tests/apply_e2e_test.rs` — stub `SubprocessRunner` that records args + returns canned exit codes; verify audit chain has expected events.
- `cli/tests/apply_smoke_test.rs` — invoke `terrashift apply` against a fixture HCL dir with stub broker + stub runner; assert summary print + audit count.

#### Article XIII rule 7 invariant test
- `libs/creds/tests/no_disk_secret_test.rs` — assert that no `std::fs::write` call in `libs/creds/src/` writes a `Credential::expose_secret()` value.

## Dependencies (added to workspace `Cargo.toml`)

- `aws-sdk-sts = "1"` — only feature: `behavior-version-latest`.
- `google_cloud_auth = "0"` — minimal feature set; check single-binary size impact.
- `azure_identity = "0"` — minimal.
- (Maybe) `tokio-stream = "0"` for line-streaming if not already present.

Pre-flight check: do these crates support `rustls-tls` (Article VI)? AWS SDK supports rustls via `feature = "rustls"`. Google + Azure may need explicit configuration.

## Test plan ordering

1. Unit tests first (`cargo test -p terrashift-creds --lib` + `cargo test -p terrashift-engine --lib`).
2. Workspace tests (`cargo test --workspace`).
3. Build (`cargo build -p terrashift`).
4. Manual smoke: `terrashift apply <empty-fixture> --no-sandbox --cloud aws` with mock STS — plan succeeds, apply denied (no `--approve`).
5. Manual real: `terrashift apply ./fixtures/e2e-aws-to-azure-azurerm/` with operator's real Azure creds (operator-side test only).

## Backward compatibility notes

- **CLI surface** — `terrashift apply` is new; no existing callers.
- **Profile schema** — `[profiles.X.creds.*]` blocks are new and optional. Existing profiles continue to work for `migrate` (which doesn't need creds).
- **`StubBroker`** — preserved. Tests still use it; only production CLI path uses real backends.

## Observability

- New `tracing::info!` spans: `executor.apply`, `runner.run`, `creds.resolve.{aws,gcp,azure}`.
- Each span carries `run_id` for audit correlation.
- Error variants carry the underlying provider error message (truncated to 200 chars to avoid log bloat).

## Order of file changes during implementation

1. `libs/shared/profile.rs` — add `CredConfig` + `CredMode` (smallest, no consumers yet).
2. `libs/creds/src/aws.rs` — STS AssumeRole impl + tests.
3. `libs/creds/src/gcp.rs` — ADC impl + tests.
4. `libs/creds/src/azure.rs` — SP env + MI impl + tests.
5. `libs/creds/src/broker.rs` — `resolve_for_cloud` dispatcher.
6. `libs/engine/src/executor/runner.rs` — new file; `SubprocessRunner` + `LocalRunner` + `DockerRunner` + tests.
7. `libs/engine/src/executor/audit_emit.rs` — new helper.
8. `libs/engine/src/executor/mod.rs` — wire runner into `apply()`.
9. `cli/src/commands/apply.rs` — new file.
10. `cli/src/main.rs` — clap subcommand.
11. `cargo fmt` + `cargo clippy --workspace --all-targets` clean before commit.
12. `cargo test --workspace`.
13. Manual smoke.
