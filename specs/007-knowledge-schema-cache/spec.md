# Spec — P-07: Knowledge schema cache

**Stage:** 1 | **P-NN:** P-07 | **Tier:** All
**TERRASHIFT_MAPPING.md:** §A row 6 (SessionStorage seam — adapted for schemas)
**Constitution:** Article VI (knowledge layer integrity — version pinning), X
**Source pattern:** `refs/stakpak/libs/api/src/storage.rs` (SessionStorage trait)

## Goal

Local-first store for provider schemas. Mapper (P-05) hits cache; on miss
fetches from `registry.terraform.io` (Stage 1: stub fetcher; real HTTP in
S17 RAG production). Validator (P-06) reads only from cache — Article III
gate requires version-pinned schemas (Article VI).

## Stage 1 scope

- `SchemaStore` trait (mirrors Stakpak's `SessionStorage` shape — async
  methods, `Send + Sync`, typed error)
- `LocalSchemaStore` SQLite-backed impl via `sqlx`
- `register_provider_schema()` — load schemas from in-process fixtures
  (test seeding; real HTTP fetch deferred to S17)
- `search_mappings()` returns `Vec::new()` (RAG stub for Mapper API stability)
- Cache invariant: pinned `(provider, version)` never expires

## Out of scope (later sessions)

- HTTP fetch to `registry.terraform.io` (S17)
- LanceDB vector store (S17)
- Per-tenant boost / telemetry-driven priority refresh (S30+)
- Offline tarball mode (S22+)

## Success criteria

- `cargo test -p terrashift-knowledge` passes:
  - Cache + retrieve schema round-trip
  - List versions returns sorted versions
  - Cache miss returns `SchemaError::NotFound` with provider+version
  - `search_mappings()` returns empty Vec (Stage 1 stub contract)
- All 4 build gates green
