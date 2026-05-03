# Analysis — P-10 (post-implementation)

## Decisions taken at implementation time

### A. `Credential` custom `Debug` impl, not auto-derived

The default `#[derive(Debug)]` would echo the inner `Zeroizing<String>`
in any debug print or panic message — a security regression. Manual
`fmt::Debug` writes `"<redacted>"` for the inner field while preserving
`resolution_method` + `expires_at` for diagnostic value. Test
`credential_debug_does_not_leak_value` asserts the raw value cannot
appear in `format!("{cred:?}")`.

### B. `audit_fetch` is the ONLY emission point in `libs/creds`

`stub.rs::audit_fetch(cred_ref, method)` is called from `resolve()`
with `&format!("{{{{secret:{name}}}}}")` — the format string is the
ONLY way to construct the audit `cred_ref` field, and `name` is the
broker's reference key, never the resolved value. Cloud brokers
(`aws.rs`/`gcp.rs`/`azure.rs`) emit no audit entries (they short-
circuit to `NotImplementedYet`). Single point of construction →
single point of audit; security-auditor agent verified the trace.

### C. `SubstitutionMap::rebuild()` longest-first iteration (security-auditor C2 fix)

The naive `str::replace(value, reference)` per `BTreeMap` entry would
over-replace if a short canned value (e.g., `"v"` in a test fixture)
happens to also appear elsewhere in the substituted text. The
auditor flagged this as LOW severity — benign for Stage 1's
audit-only consumer of rebuilt text, since real STS tokens are high-
entropy and unlikely to collide with non-secret substrings. We
implemented the easy fix (sort entries by value length descending)
to eliminate the prefix-shadowing class of issues now rather than
defer. Stage 5+ may further switch to span-based reconstruction.

### D. `pre_llm_check` thin wrapper, not a fork of the gitleaks regex set

The audit crate (P-11) already ships `terrashift_audit::scrubber::scan`
with the canonical 5 patterns + entropy heuristic. `libs/creds/src/scrub.rs`
imports and re-exports rather than duplicating. Single source of
truth; the audit-writer's panic on detection (P-11) is the *runtime*
last-line-of-defence, this `pre_llm_check` is the *outbound* proactive
layer. Two layers of defence per Article XIII rule 5.

### E. Cloud brokers' `NotImplementedYet` arms are arm-named per cross-call

`AwsBroker::fetch_gcp(...)` returns
`NotImplementedYet { which: "aws_broker_called_for_gcp", session: "S5" }`
— diagnostic-friendly: the operator knows they wired the wrong broker
for the wrong cloud, not just a generic "not implemented". Article IV
loud + named.

### F. `LocalAuditStore::register_run` (NOT `start_run`) is the public API

Initial implementation guessed `start_run`; the audit store's actual
method is `register_run(run_id, verify_key_bytes: [u8; 32])`. Test
fix routes through `AuditStore::register_run(&store, run_id,
signer.verify_key_bytes())`. Article II reference fidelity (we use
the audit crate as it actually exists, not as we'd hoped).

## Stakpak / Claude Code reference fidelity

- **Pattern**: `stakpak_arch.md §27` (secret detection / redaction;
  broker as single source of truth at tool-execution boundary).
- **Reused source**: `terrashift_audit::scrubber` for gitleaks
  patterns (P-11 `4545065`); zero duplication.
- **Signing key discipline**: `Arc<SessionSigner>` clone-share;
  matches Stakpak's per-migration keypair pattern from
  `refs/stakpak/libs/agent-core/src/checkpoint.rs` (P-11 mirror).

## What's NOT here (Stage 1 deferrals — explicit, named)

- **Real STS / ADC / managed-identity** — `NotImplementedYet { session: "S5" }`. Needs cloud test creds + `aws-sdk-rust` / `gcp-auth` / `azure-identity` crates.
- **Privacy-mode patterns** (IPs, account IDs, project IDs, subscription GUIDs) — deferred to S30 per pre-flight.
- **Disk-backed credential store** — Stage 1 is in-memory only; S5+ may add `~/.terrashift/config.toml` keyring loader (Article XIII rule 10 enforcement at the disk-IO boundary).
- **`{{secret:NAME}}` resolution at MCP-proxy boundary** — Stage 1 ships the substitution mechanism; the actual MCP proxy hook lives in `libs/mcp/proxy` and gets wired in S5+.
- **Span-based `SubstitutionMap` reconstruction** — security-auditor C2 follow-up; longest-first iteration covers the practical case for Stage 1.

## security-auditor agent results

- §A Article V: PASS (4 findings)
- §B Article XIII rules 5/7/10: PASS (3 findings)
- §C Substitution attack surface: PASS + 1 LOW (C2, fixed inline)
- §D Audit `cred_ref` invariant: PASS (single emission point traced)
- §E Verdict: **APPROVE for ship**. Zero HIGH or MEDIUM findings.

## Spec criteria coverage

| Criterion | Test |
|---|---|
| SC-001 ≥8 offline tests | 15 tests (well above target) |
| SC-002 substitution roundtrip | `substitution_roundtrip` |
| SC-003 scrubber blocks AKIA + no value echo | `pre_llm_check_blocks_aws_key` + `pre_llm_check_error_does_not_leak_raw_value` |
| SC-004 zeroize on drop | `credential_drop_zeroes_inner_string` (Zeroizing's Drop is type-system-enforced) |
| SC-005 NotImplementedYet for cloud brokers | `aws_broker_returns_not_implemented_until_s5` etc. |
| SC-006 stub broker audits + Article V invariant | `stub_broker_audits_on_fetch` (asserts serialized entry doesn't contain raw value) |
| SC-007 clippy clean | enforced by workspace gates |
| SC-008 security-auditor pass | APPROVE verdict above |
