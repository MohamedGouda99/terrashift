# Feature Specification: Credential broker — Article V cornerstone

**Feature Branch**: `010-credential-broker`
**Created**: 2026-05-02
**Status**: Draft (Stage 1 structural — `StubBroker` + scrubber +
substitution + zeroize ship today; real STS / ADC / managed-identity
calls return `NotImplementedYet` until S5 close).
**Input**: P-10 from `terrashift_prompts.md` lines 552-598 (verbatim
in Appendix A).
**Reference**: `terrashift_plan.md` §8 (auth & credentials),
`Terrashift_Plan.docx` §8 same content longform. Source patterns:
`refs/stakpak/libs/shared/src/secrets/secret_manager.rs` (redaction
patterns / entropy thresholds), `refs/stakpak_arch.md §27` (secret
detection / redaction). Internal reuse:
`terrashift-audit::scrubber::scan` (P-11 commit `4545065`) supplies
the gitleaks regex set + entropy heuristic — credential broker reuses
it rather than fork.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — LLM never sees a raw cloud credential (Priority: P1) 🎯 MVP

The Mapper, Generator, and Recovery agents work with credential
*references* like `{{secret:aws-prod-deploy}}`. When the Executor
(P-09) is about to run `terraform apply`, the credential broker
substitutes the reference for the real short-lived token at the
tool-execution boundary. The token is `Zeroizing`-wrapped so it
clears from memory after use. The LLM never sees the value.

**Why this priority**: This is the spine of Article V. Every other
security property (audit trail, redaction, sandboxing) builds on it.

**Independent Test**: Construct a `StubBroker` with one canned
credential. Build a tool-call payload containing `"{{secret:test}}"`.
Run substitution. Assert (a) the substituted output contains the
canned value, (b) the original payload is unchanged, (c) the secret
*reference* is preserved in the audit log entry but the value is not.

**Acceptance Scenarios**:
1. **Given** `StubBroker` keyed `"aws-prod" → "ASIAEXAMPLE..."`, **When** `substitute("aws sts get-caller --token {{secret:aws-prod}}")`, **Then** result contains `"ASIAEXAMPLE..."` AND the substitution round-trip is reversible (the *reference* `"{{secret:aws-prod}}"` is recoverable from the substitution map).
2. **Given** `substitute("{{secret:nonexistent}}")`, **When** broker has no match, **Then** result is `Err(CredsError::UnknownReference("nonexistent"))` — Article IV loud failure, never a silent fallthrough.
3. **Given** any `Credential` value, **When** the value is dropped, **Then** the underlying bytes are zeroed (verified via `Zeroize` semantics).

### User Story 2 — Pre-LLM scrubber blocks raw secrets (Priority: P1)

Before any payload reaches the LLM (Mapper, Recovery agent, etc.),
the broker scans every string for gitleaks-detected patterns
(AKIA*, AIza*, ghp_*, private-key headers, high-entropy tokens). On
detection: refuse to forward — return `CredsError::SecretsDetected`
with the pattern name + field path, never echoing the raw value.
This is Article XIII rule 5's structural defence (the audit log's
panic-on-detection in P-11 is the *runtime* last-line-of-defence;
this is the proactive layer that catches the bug before it reaches
either layer).

**Why this priority**: Prevents the "Mapper hallucinated a literal
key into the prompt" failure mode. Cheap to ship, catches a class
of bugs that would otherwise leak.

**Independent Test**: Construct a tool-call payload containing
`"AKIAIOSFODNN7EXAMPLE"`. Pass through `pre_llm_check`. Assert it
returns `Err(CredsError::SecretsDetected { pattern_name: "aws_access_key", ... })`.

**Acceptance Scenarios**:
1. **Given** a payload `"echo AKIAIOSFODNN7EXAMPLE"`, **When** `pre_llm_check`, **Then** `Err(SecretsDetected)` with `pattern_name = "aws_access_key"`.
2. **Given** a clean payload `"echo {{secret:aws-prod}}"` (only references), **When** `pre_llm_check`, **Then** `Ok(())`.

### User Story 3 — Cloud provider brokers stub-out cleanly until S5 close (Priority: P2)

`AwsBroker::fetch_aws(role)` / `GcpBroker::fetch_gcp(account)` /
`AzureBroker::fetch_azure(subscription)` are wired *today* — they
return `Err(CredsError::NotImplementedYet)` with a clear message
naming which session unblocks them. This makes the broker trait
surface complete from day one; the implementation is a one-method
swap once cloud creds + Docker are available.

**Why this priority**: Lets P-09 (Executor) compile against a
complete broker surface; the failure mode is loud + named (not
panic, not silent stub returning fake data).

**Independent Test**: `AwsBroker::new().fetch_aws("test-role").await`
returns `Err(CredsError::NotImplementedYet { which: "aws_sts_assume_role", session: "S5" })`.

**Acceptance Scenarios**:
1. **Given** `AwsBroker` instance, **When** `fetch_aws(role)`, **Then** `Err(NotImplementedYet)` naming `S5` as the unblocker.
2. Same for `GcpBroker::fetch_gcp` and `AzureBroker::fetch_azure`.

### User Story 4 — Every credential operation is audited (Priority: P2)

When any broker fetches a credential (real or stub), an
`AuditPayload::CredentialResolution` entry is appended with `cred_ref`
(the reference, not the value), `resolution_method` (e.g.
`"sts_assume_role"`, `"stub"`), and `expires_at`. Article V invariant
("every credential operation is audited") becomes structurally
enforceable.

**Independent Test**: With an `Arc<dyn AuditStore>` injected into
`StubBroker`, calling `fetch_aws("role")` appends one
`CredentialResolution` entry with the right `cred_ref` and method.

### Edge Cases

- **Empty reference** `{{secret:}}` — `Err(CredsError::MalformedReference)`.
- **Nested references** `{{secret:foo{{secret:bar}}}}` — Stage 1 rejects (single-pass substitution; nested refs would require recursion + risks infinite loop). `Err(CredsError::MalformedReference)`.
- **Reference with whitespace** `{{secret: aws-prod }}` — Stage 1 rejects (strict). Tighter is safer.
- **Multiple references in one string** `"{{secret:aws}} {{secret:gcp}}"` — both substitute correctly in one pass.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST expose `pub trait CredentialBroker` async with three methods: `fetch_aws(role: &str)`, `fetch_gcp(account: &str)`, `fetch_azure(subscription: &str)`. Each returns `Result<Credential, CredsError>`.
- **FR-002**: `Credential` MUST wrap the secret value in `zeroize::Zeroizing<String>` (or `Vec<u8>`) so dropping the value zeroes the memory.
- **FR-003**: System MUST ship `StubBroker` for tests + dev — canned `(name → Credential)` map; never touches the network.
- **FR-004**: `AwsBroker` / `GcpBroker` / `AzureBroker` MUST be present (compile-ready), each returning `Err(CredsError::NotImplementedYet { which, session: "S5" })` until cloud creds are wired.
- **FR-005**: System MUST implement `substitute(payload, broker) -> Result<String, CredsError>` that finds every `{{secret:NAME}}` reference and replaces with the broker's resolved value. Returns the substituted string + a SubstitutionMap recording which references were resolved (for audit emission).
- **FR-006**: `pre_llm_check(payload)` MUST scan for secret patterns via `terrashift-audit::scrubber::scan`. On match, return `Err(CredsError::SecretsDetected { pattern_name, field_path })`. Article XIII rule 5 enforcement at the *outbound* boundary (audit-writer panic is the inbound boundary).
- **FR-007**: Every successful broker `fetch_*` call MUST append `AuditPayload::CredentialResolution` to the injected `AuditStore` if one is present. The `cred_ref` is the *reference* string (never the value).
- **FR-008**: All errors MUST be named in `CredsError` enum: `UnknownReference(String)`, `MalformedReference(String)`, `SecretsDetected { pattern_name, field_path }`, `NotImplementedYet { which, session }`, `Audit(AuditError)`.
- **FR-009**: Per Article XIII rule 10, `~/.terrashift/config.toml` is the only sanctioned credential location. Stage 1 doesn't load creds from disk yet (handled in S5 / P-09 + P-10 close), but the trait surface accommodates: `StubBroker` is constructed in-memory from `Vec<(String, String)>`, never reading disk.

### Key Entities

- **`Credential`** — `Zeroizing<String>`-wrapped; `Drop` clears memory.
- **`CredentialBroker`** trait — `fetch_aws/gcp/azure` async surface.
- **`StubBroker`** — Stage 1 in-memory canned-credential impl.
- **`AwsBroker` / `GcpBroker` / `AzureBroker`** — Stage 1 stubs returning `NotImplementedYet`; S5 wires real STS/ADC/managed-identity.
- **`SubstitutionMap`** — `HashMap<reference, ()>` recording which refs were resolved per call (used for audit + the *reverse* path: rebuilding original from substituted string + map).
- **`CredsError`** — `thiserror`-derived enum.
- **Re-export of `terrashift-audit::scrubber::scan`** — single source of truth for gitleaks patterns; `pre_llm_check` is a thin wrapper.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test -p terrashift-creds` runs ≥ 8 offline tests covering FR-001 through FR-009; all pass without network access.
- **SC-002**: Substitution round-trip — `substitute("...{{secret:test}}...", broker)` returns substituted text; reverse rebuild via `SubstitutionMap` recovers the original reference string.
- **SC-003**: `pre_llm_check` blocks `AKIAIOSFODNN7EXAMPLE` with `pattern_name = "aws_access_key"`. Error message names the pattern; raw value NEVER echoed (verified by string `assert!(!format!("{err}").contains("AKIAIOSFODNN7EXAMPLE"))`).
- **SC-004**: `Credential` zeroes memory on drop. Verified via `Zeroize` invariant test (the underlying buffer post-drop is zero bytes).
- **SC-005**: `AwsBroker::fetch_aws("any")` returns `Err(NotImplementedYet { which, session: "S5" })`. Same for GCP / Azure.
- **SC-006**: `StubBroker::fetch_aws("test-role")` returns `Ok(Credential)` with the canned value AND emits one `AuditPayload::CredentialResolution` entry to the injected audit store.
- **SC-007**: Production paths in `libs/creds/src/` are clippy `unwrap_used` / `expect_used` / `string_slice` clean (Article XIII rule 3).
- **SC-008**: `security-auditor` agent runs cleanly against the staged diff — verifying no raw cred handling outside the broker boundary, no Article V / Article XIII rule 5 / 7 / 10 violations.

## Assumptions

- **`terrashift-audit::scrubber::scan` is stable** (P-11 commit `4545065`); reused as the gitleaks pattern source.
- **`zeroize` is in workspace deps** (verified: `zeroize = { version = "1", features = ["zeroize_derive"] }` at root `Cargo.toml:90`).
- **Real STS / ADC / managed-identity wiring** lands in S5 close (needs cloud test creds + `aws-sdk-rust` / `gcp-auth` / `azure-identity` crates). Stage 1's `NotImplementedYet` arms are explicit handoffs, not silent stubs.
- **`AuditStore` injection is optional** at Stage 1; tests that don't care about audit pass `None`. P-11's `LocalAuditStore` is the canonical impl.

## Appendix A — Canonical P-10 prompt (verbatim from `terrashift_prompts.md:552-598`)

```text
P-10 — Credential broker

When to use: Together with P-09.
Reference sections: stakpak_arch.md section 27 (secret detection /
redaction), section 9 (shared crate where secret types live).

Implement the credential broker in libs/creds. Single source of
truth for any operation needing cloud credentials.

PRIMARY REFERENCE: stakpak_arch.md section 27 covers Stakpak's
secret detection and redaction. Pattern: gitleaks rule set + entropy
filter detect API keys, tokens, certificates. Detected secrets
replaced in-place with stable tokens [REDACTED_SECRET:rule:6char_id];
redaction map persisted at .stakpak/session/secrets.json.

We adopt this directly with terrashift-prefix tokens.

IMPLEMENT:
1. pub trait CredentialBroker — async trait with fetch_aws(role),
   fetch_gcp(account), fetch_azure(subscription). Each returns
   short-lived credentials.
2. AwsBroker — uses STS AssumeRole; never holds long-term keys
3. GcpBroker — uses Application Default Credentials with workload
   identity federation when available
4. AzureBroker — uses managed identity or service principal with
   short-lived tokens
5. Secret substitution: LLM works with references like
   {{secret:aws-prod-deploy}}; broker resolves at execution time only
6. Pre-prompt scrubber: gitleaks-style patterns + entropy-based
   detection of high-entropy strings; replaces with tokens before LLM
   context (per section 27)
7. Every fetch audit-logged with timestamp + invoking operation
8. Credentials zeroized in memory after use using zeroize crate

DESCEND INTO SOURCE: For Stakpak's secret manager, read
refs/stakpak/libs/shared/src/secrets/ and secret_manager.rs.

PRIVACY MODE: Per section 27, --privacy-mode adds IP addresses,
AWS account IDs, PII patterns. We adopt and add cross-cloud-specific
patterns (GCP project IDs, Azure subscription GUIDs).

TESTS:
- Mock STS, confirm short-lived tokens fetched and dropped
- Confirm raw credentials never appear in any returned ToolCall struct
- Confirm audit log entries
- Confirm zeroize called on credential drop
- Confirm secret substitution roundtrips correctly

CONSTITUTION CHECKS:
- Article V — heart of it. Cite in all tests and PR description.
- Article XIII rule 5 (don't bypass redaction layer)
- Article XIII rule 7 (don't write disk-bound secrets)
- Article XIII rule 10 (don't store API keys outside ~/.terrashift/)
```

**Stage 1 deviation**: real STS / ADC / managed-identity calls are
deferred to S5 close (requires cloud test creds + `aws-sdk-rust` /
`gcp-auth` / `azure-identity` crates). Stage 1 ships the trait
surface + `StubBroker` + scrubber + substitution + audit hook +
zeroize wrapper — when S5 close arrives, three methods swap from
`NotImplementedYet` to real STS/ADC/managed-identity.

Privacy-mode patterns (IPs, account IDs, project IDs, subscription
GUIDs) are deferred to S30 (per pre-flight + P-11 clarify Q5: full
gitleaks rule set arrives there). Stage 1 reuses P-11's existing
patterns (AWS, GCP, GitHub, private-key, high-entropy).

## Appendix B — Spec format

This is the third P-NN's spec to use the official Spec Kit
template (after P-03 and P-05). Specs 000-014 use the legacy
Goal/Scope shape.
