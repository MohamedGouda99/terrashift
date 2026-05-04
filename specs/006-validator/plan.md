# Plan — P-06

## Files

| File | Purpose | Lines (est) |
|---|---|---|
| `libs/engine/src/validator/mod.rs` | UPDATE: `Validator`, `validate`, public API | ~120 |
| `libs/engine/src/validator/errors.rs` | NEW: `ValidatorError` (Knowledge wrap) | ~30 |
| `libs/engine/src/validator/report.rs` | NEW: `ValidationReport`, `ValidationError`, `ValidationWarning` | ~120 |
| `libs/engine/tests/validator_test.rs` | NEW: 7+ tests covering all spec criteria | ~250 |

**Total est:** ~520 lines (production ~270 + tests ~250). Within Article VII.

## Build order

1. `errors.rs` (no internal deps)
2. `report.rs` (no internal deps)
3. `mod.rs` rewrite (deps: `errors`, `report`, `mapper::MappingPlan`, `terrashift-knowledge::KnowledgeService`)
4. `tests/validator_test.rs`
5. `cargo check -p terrashift-engine --tests`
6. `cargo clippy --workspace --all-targets -- -D warnings`
7. `cargo fmt -- --check`
8. `cargo test -p terrashift-engine`
9. `cargo test --workspace`
10. `code-reviewer` agent → resolve findings
11. `constitution-checker` agent → resolve findings
12. Commit

## Cargo dep updates

`libs/engine/Cargo.toml` already has `terrashift-knowledge`,
`tokio`, `tempfile`, etc. Nothing new needed.

## Public API shape

```rust
pub struct Validator {
    knowledge: Arc<KnowledgeService>,
}

impl Validator {
    pub fn new(knowledge: Arc<KnowledgeService>) -> Self;

    /// Validate every target resource in `plan` against the schema for
    /// `(plan.target_provider, target_version)`. Always walks the entire
    /// plan; returns aggregate errors + warnings.
    pub async fn validate(
        &self,
        plan: &MappingPlan,
        target_version: &str,
    ) -> Result<ValidationReport, ValidatorError>;
}

pub struct ValidationReport {
    pub passed: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

pub enum ValidationError {
    UnknownResourceType { addr: String, target_type: String },
    UnknownAttribute { addr: String, attr: String, target_type: String },
    MissingRequiredAttribute { addr: String, attr: String, target_type: String },
}

pub enum ValidationWarning {
    DeprecatedAttribute { addr: String, attr: String, since: String },
    SetComputedAttribute { addr: String, attr: String, target_type: String },
}
```

## Test fixture strategy

`libs/knowledge` ships `StubSchemaFetcher` (P-07). Tests construct a
`KnowledgeService` with `(LocalSchemaStore, InMemoryVectorStore,
StubEmbeddingService, StubSchemaFetcher)` over a `tempfile::tempdir()`,
seed it with one or two synthetic `ProviderSchema`s, then invoke
`Validator::validate` against hand-crafted `MappingPlan`s.

## Citation pattern

```rust
//! Pattern: Terrashift-specific (no Stakpak counterpart — migration domain).
//!         Closest analog is stakpak_arch.md §27 (single redaction enforcement
//!         point), which we mirror in spirit: one place where the AI-safety
//!         invariant gets enforced.
//! Constitution: Article III (every LLM-emitted attribute checked against
//!               schema), Article IV (validation errors are loud), Article XIII
//!               rule 3 (no unwrap/expect/string-slice).
```

## Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| Test fixture KnowledgeService too heavy to set up per-test | Medium | Build a small `make_test_knowledge` helper in tests; ~30 LOC, reused across all 7 tests |
| `LocalSchemaStore` async setup races concurrent tests | Low | Each test uses a fresh `tempfile::TempDir` for the SQLite file |
| Validator-Knowledge async lifecycle (Arc + tokio) | Low | `KnowledgeService` already returns `Arc`-friendly types; pattern matches P-08 |
