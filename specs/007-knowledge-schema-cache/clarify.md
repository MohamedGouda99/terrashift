# Clarifications — P-07

User-on-behalf decisions per constitution + plan.

## Q1: SQLite file vs in-memory for Stage 1?

**Decision:** **In-memory by default + file-based via opt-in builder.** Tests
run against `:memory:`. CLI consumer creates with `LocalSchemaStore::new(path)`.
Mirrors Stakpak's `SessionStorage` allowing both in-process and persisted impls.

## Q2: Seeding test schemas — embed in Rust constants or separate fixture files?

**Decision:** **Rust constants in `tests/fixtures.rs`**. Stage 1 has 2-3 hand-
written minimal schemas (one for `aws_vpc`, one for `azurerm_virtual_network`).
Avoiding extra file reads keeps tests hermetic.

## Q3: `MappingExample` shape for `search_mappings()` stub?

**Decision:** **Define the type with all fields but return `Vec::new()`**.
S17 (RAG production) populates the impl; the API stays stable.

```rust
pub struct MappingExample {
    pub source_resource: String,   // "aws_vpc"
    pub target_resource: String,   // "azurerm_virtual_network"
    pub attribute_alignments: Vec<(String, String)>,
    pub confidence: f64,
    pub source_provider: String,
    pub target_provider: String,
}
```

## Q4: Schema persistence format — JSON column, normalized tables, or both?

**Decision:** **JSON column** in Stage 1. Simpler queries, single-row write
per (provider, version). Normalized tables (one row per resource, one per
attribute) deferred to S17 when query patterns get richer.

## Q5: Article VI (version pinning) enforcement at insert time?

**Decision:** **Yes — INSERT OR IGNORE on (provider, version) PK**. Once a
pinned version is stored, a re-fetch overwrite is rejected silently. The
caller learns via `cache_schema()` returning `Ok(was_inserted: false)` —
explicit signal, not silent failure.
