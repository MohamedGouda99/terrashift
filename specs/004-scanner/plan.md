# Implementation Plan — P-04

## Module structure (libs/engine/src/scanner/)

| File | Purpose | Lines (est) |
|---|---|---|
| `mod.rs` | Re-exports, top-level `Scanner` struct + `scan()` entry point | ~60 |
| `inventory.rs` | EstateInventory + ScannedFile + Resource/Provider/Module/etc types | ~120 |
| `parser.rs` | Per-file parsing — `hcl::from_str` → typed entities | ~150 |
| `walker.rs` | Directory walking — `walkdir` over `.tf` files | ~50 |
| `errors.rs` | `ScannerError` enum (UnsupportedFeature, ParseError, IoError) | ~40 |
| `tests/` (in `mod.rs`) | Unit tests for parser + walker | ~80 |

## Dependencies needed

| Crate | Already in workspace? | Use |
|---|---|---|
| `hcl-rs` | ✅ Yes (workspace dep) | Parsing |
| `walkdir` | ❌ Add | Recursive `.tf` discovery |
| `thiserror`, `anyhow`, `tracing`, `serde` | ✅ | Standard |

Add to workspace Cargo.toml: `walkdir = "2"`. Then engine/Cargo.toml: `walkdir = { workspace = true }`.

## Build order

1. Add `walkdir` to workspace + engine Cargo.toml
2. Write `errors.rs` (no deps)
3. Write `inventory.rs` (no internal deps)
4. Write `parser.rs` (deps: errors, inventory)
5. Write `walker.rs` (deps: errors)
6. Write `mod.rs` (deps: all above; exports `Scanner`, `scan`)
7. Update `libs/engine/src/lib.rs` — pub use scanner re-exports
8. Write `libs/engine/tests/scanner_test.rs` — integration tests
9. `cargo check -p terrashift-engine`
10. `cargo test -p terrashift-engine`
11. `cargo clippy --all-targets -- -D warnings`
12. `cargo fmt -- --check`
13. Commit

## Public API

```rust
// libs/engine/src/scanner/mod.rs
pub use self::errors::ScannerError;
pub use self::inventory::{EstateInventory, ScannedFile, Resource, Provider,
                          Module, Variable, Output, DataSource, SourceSpan};

pub struct Scanner;

impl Scanner {
    /// Walk `root` recursively, parse every `.tf` file, return inventory.
    /// Returns Err on first hard-fail (e.g., `dynamic` block found).
    pub fn scan(root: &Path) -> Result<EstateInventory, ScannerError> { ... }

    /// Parse a single `.tf` file content. Used by `scan()` and tests.
    pub fn parse_file(path: &Path, content: &str) -> Result<ScannedFile, ScannerError> { ... }
}
```

## Constitution

- Article I — Scanner is deterministic, not an agent
- Article IV — fail loudly on `dynamic` blocks; capture other "Stage 5"
  features in `scan_notes`
- Article XIII rule 3 — no `unwrap()`/`expect()`/`&s[..n]` (verified by clippy)

## Citation

Each source file cites:
```rust
//! Pattern: terrashift_plan.md §5 (HLD-2 pipeline component 1), §11 (Terraform completeness).
//! No Stakpak counterpart — Scanner is Terrashift-specific.
//! Constitution: Article I, IV, XIII rule 3.
```
