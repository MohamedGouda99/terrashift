# Implementation Plan: Credential broker

**Branch**: `010-credential-broker`
**Date**: 2026-05-02
**Spec**: `specs/010-credential-broker/spec.md`

## Summary

Ship the Article V cornerstone: trait + StubBroker + zeroize wrapper +
substitution + scrubber-as-pre-LLM-check + audit hook. Real
STS / ADC / managed-identity are S5-close work blocked on cloud creds.
The structural code makes Article V *enforceable* via type system and
mandatory `security-auditor` review.

## Technical Context

**Language/Version**: Rust 1.94.1
**Primary Dependencies**: `zeroize`, `tokio`, `async-trait`,
`thiserror`, `tracing`, `serde`, `terrashift-audit` (provides scrubber
+ AuditPayload). All present in `libs/creds/Cargo.toml`.
**Storage**: N/A — broker is stateless apart from `StubBroker`'s
in-memory canned map.
**Testing**: `cargo test -p terrashift-creds` (offline; no network).
**Target Platform**: workspace library crate.
**Project Type**: library extending the existing `libs/creds/` crate.
**Performance Goals**: substitution is O(n) over the input string;
scrubber is bounded by regex + entropy heuristic from P-11. Both
microseconds for typical payloads.
**Constraints**: Article V is load-bearing — every code path must be
auditable. Article XIII rule 3 (no panics in production). Real cred
fetch is `NotImplementedYet`; the trait surface is complete.
**Scale/Scope**: ~500 LOC production + ~300 LOC tests.

## Constitution Check

*GATE: Must pass before implementation. Re-check before commit via
`security-auditor` subagent.*

- ✓ **Article I** — broker is deterministic infra, NOT an agent.
- ✓ **Article II** — `terrashift-audit::scrubber` reused (no fork);
  `stakpak_arch.md §27` cited in module head; verbatim Sha256 idiom
  from P-11.
- ✓ **Article IV** — every `CredsError` named; `NotImplementedYet`
  is loud + names the unblocking session.
- ✓ **Article V** — heart. The trait surface forces every cred fetch
  to go through the broker; the substitution mechanism keeps the LLM
  off raw values; the scrubber catches accidents at the outbound boundary;
  `Zeroizing<Credential>` clears memory; audit hook records every fetch.
- ✓ **Article XIII rule 3** — production grep clean.
- ✓ **Article XIII rule 5** — `pre_llm_check` is the broker-side
  scrub layer; combined with the audit-writer's panic-on-detection
  (P-11), this gives two-layer defence.
- ✓ **Article XIII rule 7** — no disk-bound secret writing in Stage 1.
  `StubBroker` is in-memory only.
- ✓ **Article XIII rule 10** — Stage 1 doesn't load creds from disk
  yet; when S5 wires the disk-load path, `~/.terrashift/config.toml`
  is the only sanctioned location (per pre-flight Decision 9 sample).

## Project Structure

### Documentation (this feature)

```text
specs/010-credential-broker/
├── plan.md           # this file
├── spec.md           # User Stories + FRs + SCs
├── tasks.md          # Phase-organized
├── analyze.md        # post-impl
└── checklist.md      # final gate
```

### Source Code (repository workspace)

```text
libs/creds/src/
├── lib.rs            # UPDATE: re-exports
├── broker.rs         # CredentialBroker trait + Credential (Zeroizing)
├── stub.rs           # StubBroker (in-memory canned map)
├── aws.rs            # AwsBroker (NotImplementedYet until S5)
├── gcp.rs            # GcpBroker (NotImplementedYet until S5)
├── azure.rs          # AzureBroker (NotImplementedYet until S5)
├── substitution.rs   # {{secret:NAME}} substitution + SubstitutionMap
├── scrub.rs          # pre_llm_check thin wrapper around terrashift-audit::scrubber
└── errors.rs         # CredsError

libs/creds/tests/
└── creds_test.rs     # 8+ offline tests
```

**Structure Decision**: standard Rust library; one module per orthogonal
concern. Mirror Scanner / Audit / Eval module shapes.

## Complexity Tracking

> No constitution violations. Two deliberate Stage 1 deferrals:
>
> 1. **Real STS / ADC / managed-identity** — `NotImplementedYet`
>    until S5; needs cloud test creds + provider SDK crates.
> 2. **Privacy-mode patterns** (IPs, account IDs, project IDs,
>    subscription GUIDs) — deferred to S30. Stage 1 reuses P-11's
>    existing 5 patterns; that's "minimal but sufficient" per P-11
>    clarify Q5.
