# Spec — P-04: Scanner (deterministic HCL parser)

**Status:** in progress
**P-NN:** P-04 (terrashift_prompts.md lines 312-348)
**TERRASHIFT_MAPPING.md:** §B row 1 (migration tools), §41 Phase 2 (re-skin tool catalog)
**Constitution:** Article I (deterministic, not an agent), IV (failures loud), XIII rule 3

## Goal

Read a directory of `.tf` files. Produce a typed `EstateInventory` representing
every resource, provider, module, variable, output, and data source the
Mapper will consume. Pure deterministic Rust — no LLM, no network calls.

This is the FIRST entry in the Terrashift migration pipeline (HLD-2 box 1).
Output flows to Mapper (P-05).

## Scope (Stage 1)

**In scope:**
- Walk a `.tf` file tree (one or more directories)
- Parse each file with `hcl-rs`
- Extract: providers, modules (declarations with source path), resources,
  variables, outputs, data sources
- Capture `file_path:line` for every entity (for error reporting + audit)
- Detect Stage 1 unsupported features and fail loudly per Article IV:
  - `dynamic` blocks — defer to Stage 5 (terrashift_plan.md §11.1)
  - Provisioners (local-exec, remote-exec) — flag (Stage 5)
  - Terragrunt wrappers — flag (Stage 5)
- No expression evaluation — `var.X`, `local.Y`, `module.Z.id` references
  are captured as raw strings; resolving them is the Mapper's responsibility
- No network calls (no provider schema lookup; that's the Validator P-06)

**Out of scope (Stage 5+):**
- Resolving expression references
- Recursing into module sources (only capture the source path)
- HCL2 dynamic blocks
- Workspace state file inspection

## Reference

- **No Stakpak counterpart** — Scanner is Terrashift-specific (cross-cloud
  Terraform migration domain; Stakpak is general DevOps agent)
- `hcl-rs` crate (workspace dep already present)
- `terrashift_plan.md` §11 (Terraform completeness — what Stage 1 must preserve)
- `fixtures/aws-to-azure-real/` — real-world Terraform 0.12-era code for tests

## Success criteria

- `cargo check -p terrashift-engine` compiles
- `cargo test -p terrashift-engine` passes 5 cases:
  - Parse a single `.tf` file with one resource → 1 resource in inventory
  - Walk the fixture's `aws/modules/vpc/` → finds aws_vpc, aws_subnet (×3), aws_internet_gateway, aws_route_table, etc.
  - Parse a file with a `provider` block → captures provider type + region attribute
  - Parse a file with a `module` block → captures module source path
  - Parse a file containing a `dynamic` block → returns `ScannerError::UnsupportedFeature`
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo fmt -- --check` clean
- Public API: `Scanner::scan(root: &Path) -> Result<EstateInventory, ScannerError>`

## Public types (in `libs/shared/src/inventory.rs` if shared, else in scanner module)

```rust
pub struct EstateInventory {
    pub root_dir: PathBuf,
    pub files: Vec<ScannedFile>,
}
pub struct ScannedFile {
    pub path: PathBuf,
    pub providers: Vec<Provider>,
    pub modules: Vec<Module>,
    pub resources: Vec<Resource>,
    pub variables: Vec<Variable>,
    pub outputs: Vec<Output>,
    pub data_sources: Vec<DataSource>,
}
pub struct Resource {
    pub resource_type: String,  // "aws_vpc"
    pub name: String,           // "default"
    pub attributes: BTreeMap<String, String>,  // raw expressions as strings
    pub source_span: SourceSpan,
}
// (similar for Provider, Module, Variable, Output, DataSource)
```
