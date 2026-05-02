# Spec — P-11: Audit log

**Stage:** 1 | **P-NN:** P-11 | **Tier:** All
**TERRASHIFT_MAPPING.md:** §A row 6 (storage seam, separate from Knowledge),
§E rule 5 (the writer is the LAST line of defence for redaction).
**Constitution:** Article V (heart of credentials/security), IX (data governance),
XIII rule 3 + 5
**Source pattern:** `refs/stakpak/libs/server/src/checkpoint_store.rs` (chained-store discipline);
`refs/stakpak/libs/agent-core/src/checkpoint.rs` (envelope serialization).

## Goal

Signed, hash-chained, append-only audit log. Every meaningful operation in
Terrashift writes an entry. The writer is the LAST defence against secret
leakage (Article XIII rule 5) — pre-write scrub panics if a gitleaks-detected
pattern slips through.

## Stage 1 scope

- `AuditEntry` schema with `prev_hash`, `content_hash`, Ed25519 `signature`
- `AuditPayload` enum with 5 variants from `terrashift_plan.md` §6.X:
  - `LlmCall { provider, model_id, provider_endpoint, tier, input_tokens,
    output_tokens, cache_read_tokens, cache_write_tokens, cost_usd_micros }`
  - `ToolExecution { tool_name, cred_ref, target, duration_ms }`
  - `PhaseTransition { from, to, checkpoint_id }`
  - `CredentialResolution { cred_ref, resolution_method, expires_at }`
  - `FileOperation { path, kind, backup_path }`
- `AuditStore` trait + `LocalAuditStore` SQLite append-only impl
- `verify_chain(run_id) -> Result<(), AuditError>` — walks chain, verifies
  every signature + content hash + prev_hash linkage
- `export(run_id) -> Vec<AuditEntry>` — for compliance review
- `query(filter)` — filter by run_id / time range / payload variant
- Pre-write redaction: gitleaks regex set + entropy filter; panic on detection
- `AuditWriterHook: terrashift_agent_core::AgentHook` — emits `ToolExecution`
  payload after every tool execution

## Stage 1 out of scope

- LLM cost computation (tokens are recorded; cost_usd_micros is computed
  upstream by libs/ai once that ships in S4)
- Per-migration Ed25519 key rotation UX (Stage 6)
- SIEM-format export (Stage 6 SOC2 work)

## Success criteria

- `cargo test -p terrashift-audit` passes:
  - Append + verify chain round-trip per payload variant
  - Tampering with any field breaks `verify_chain` with the specific field flagged
  - Tampering with signature is detected separately from payload tampering
  - Append O(1); verify O(n) acceptable up to 10k entries
  - Redaction: an entry containing a raw API-key-like string is REJECTED via panic
  - Multi-run isolation: appending to run_id A doesn't affect run_id B chain
- All 4 build gates green
