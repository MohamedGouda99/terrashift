# Plan — P-07

## Files

| File | Purpose | Lines (est) |
|---|---|---|
| `libs/knowledge/src/types.rs` | ProviderSchema, ResourceSchema, AttributeSchema, MappingExample | ~80 |
| `libs/knowledge/src/errors.rs` | SchemaError enum | ~30 |
| `libs/knowledge/src/schema_store.rs` | SchemaStore trait | ~50 |
| `libs/knowledge/src/local_schema_store.rs` | LocalSchemaStore (sqlx + sqlite) | ~150 |
| `libs/knowledge/src/lib.rs` | re-exports | ~20 |
| `libs/knowledge/tests/schema_cache_test.rs` | round-trip + miss + version list | ~80 |

## Build order

1. types.rs (no deps)
2. errors.rs (no deps)
3. schema_store.rs (deps: types, errors)
4. local_schema_store.rs (deps: schema_store, sqlx)
5. lib.rs re-exports
6. Tests
7. cargo check + test + clippy + fmt
8. Commit

## SQL schema

```sql
CREATE TABLE IF NOT EXISTS provider_schemas (
    provider TEXT NOT NULL,
    version TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (provider, version)
);
CREATE INDEX IF NOT EXISTS idx_provider ON provider_schemas(provider);
```

## Cargo dep updates

- `libs/knowledge/Cargo.toml` already has sqlx — confirm runtime feature
  set is right (sqlite + macros + chrono + uuid)
- Add `chrono = { workspace = true }` if not present

## Citation

Each file cites:
```rust
//! Pattern: stakpak_arch.md §9 (SessionStorage trait shape).
//! Source: refs/stakpak/libs/api/src/storage.rs (analogous shape; ours covers schemas not sessions).
//! Constitution: Article VI (version pinning), X (tracing spans).
```
