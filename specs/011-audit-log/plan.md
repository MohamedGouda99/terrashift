# Plan — P-11

## Files

| File | Purpose | Lines (est) |
|---|---|---|
| `libs/audit/src/entry.rs` | AuditEntry struct + AuditPayload enum + Actor + Outcome | ~120 |
| `libs/audit/src/errors.rs` | AuditError enum | ~30 |
| `libs/audit/src/signer.rs` | Ed25519 keypair generation + sign + verify wrappers | ~50 |
| `libs/audit/src/scrubber.rs` | Pre-write secret detection (gitleaks-style + entropy) | ~80 |
| `libs/audit/src/store.rs` | AuditStore trait + LocalAuditStore (sqlx) | ~200 |
| `libs/audit/src/verify.rs` | Chain verification | ~80 |
| `libs/audit/src/hooks.rs` | AuditWriterHook impl AgentHook | ~70 |
| `libs/audit/src/lib.rs` | re-exports | ~20 |
| `libs/audit/tests/audit_chain_test.rs` | Round-trip + tamper + redaction-panic tests | ~150 |

## Build order

1. errors.rs (no deps)
2. entry.rs (no deps beyond serde)
3. signer.rs (deps: errors)
4. scrubber.rs (deps: errors)
5. store.rs (deps: entry, errors, signer, scrubber, sqlx)
6. verify.rs (deps: entry, errors, signer)
7. hooks.rs (deps: store + agent-core::AgentHook)
8. lib.rs re-exports
9. Tests
10. cargo check + test + clippy + fmt
11. Commit

## Cargo dep updates

`libs/audit/Cargo.toml` already has ed25519-dalek, sha2, sqlx, chrono, uuid,
serde, serde_json, thiserror, anyhow, tracing per S1 bootstrap. Add:
- `regex = "1"` for redaction patterns (or `gitleaks-rs` if available)
- `terrashift-agent-core = { workspace = true }` for AgentHook trait
- `tokio = { workspace = true }` dev-dep

## SQL schema

```sql
CREATE TABLE IF NOT EXISTS audit_runs (
    run_id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL,        -- Ed25519 verify key
    started_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_entries (
    id TEXT PRIMARY KEY,             -- entry UUID
    run_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    actor_json TEXT NOT NULL,
    operation TEXT NOT NULL,
    outcome_json TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    prev_hash BLOB NOT NULL,
    content_hash BLOB NOT NULL,
    signature BLOB NOT NULL,
    FOREIGN KEY (run_id) REFERENCES audit_runs(run_id)
);
CREATE INDEX IF NOT EXISTS idx_entries_run_time ON audit_entries(run_id, timestamp);
```

## Citation

```rust
//! Pattern: stakpak_arch.md §22 (checkpoint envelope discipline informs the
//! hash-chain shape; audit chain is INDEPENDENT of checkpoint chain per §22).
//! Source: refs/stakpak/libs/agent-core/src/checkpoint.rs (envelope shape).
//! Constitution: Article V (heart of it), IX (append-only never deleted),
//! XIII rules 3 + 5.
```
