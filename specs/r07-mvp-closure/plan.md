# Implementation Plan — r07-mvp-closure

**Spec**: `specs/r07-mvp-closure/spec.md`
**Clarify**: `specs/r07-mvp-closure/clarify.md`
**Branch**: `feat/r07-mvp-closure`
**Constitution articles in scope**: I, III, IV, XII rule 2, XIII rules 1/2/3
**stakpak_arch.md sections referenced**: §10 (LLM SDK), §27 (redaction adjacent)

## Architectural shape

Two orthogonal sub-features ship together:

1. **Mapper enhancements** — context-injection + parse-time validation. Lives
   entirely in `libs/engine/src/mapper/{prompt,mod,errors}.rs`. The `Mapper`
   struct gains an `Arc<KnowledgeService>` reference (already passed at
   call time; no new field) and uses it to fetch the target-provider schema
   for required-attribute discovery.

2. **Default-output UX** — convention-over-configuration in
   `cli/src/commands/migrate.rs`. Pure path arithmetic; no LLM, no schema.

Both compile + test independently; both ship in the same PR for review
ergonomics (single migration story).

## File changes (with line counts)

### `libs/engine/src/mapper/errors.rs` ([MODIFY], +25)

Add three error variants per FR-3, FR-4, the `TargetProviderMismatch` is
subsumed:

```rust
#[derive(Debug, Error)]
pub enum MapperError {
    // … existing variants …

    #[error("Mapper produced empty target_type for source resource '{source_addr}' — \
             rejected at parse time. Valid target types must be one of: {supported:?}")]
    EmptyTargetType {
        source_addr: String,
        supported: Vec<String>,
    },

    #[error("Mapper produced unsupported target_type '{target_type}' for source resource \
             '{source_addr}'. Supported types for this provider: {supported:?}")]
    UnsupportedTargetType {
        source_addr: String,
        target_type: String,
        supported: Vec<String>,
    },
}
```

### `libs/engine/src/mapper/prompt.rs` ([MODIFY], +80)

1. Bump `SYSTEM_PROMPT` version string `Terrashift Mapper v1 → v2`. Article
   XII rule 2: the bump invalidates the prior cache, intentionally.
2. Augment `SYSTEM_PROMPT` with two new structural blocks:
   - `VALID TARGET TYPES` — populated at prompt-build time from
     `TemplateRegistry.stage1().keys()` filtered by target-provider prefix.
   - `REQUIRED ATTRIBUTES` — populated at prompt-build time from
     `KnowledgeService::fetch_schema()` per target type.
3. New helper `build_system_prompt(target_provider, target_schema, supported_types)`
   that assembles the SYSTEM_PROMPT body with the dynamic blocks injected.

The static `SYSTEM_PROMPT` constant becomes a `const SYSTEM_PROMPT_HEADER`
(the rules portion) + a function that appends dynamic sections. Cache key
discipline: the dynamic content is part of the key (already true since
`estate_cache_key` covers the `EstateInventory`; we extend coverage to
include `target_schema_version` so swapping versions invalidates).

### `libs/engine/src/mapper/mod.rs` ([MODIFY], +60)

1. Inside `Mapper::map`, after the LLM round-trip and JSON parse, run a
   new `validate_plan(&plan, &supported_types)` pass that:
   - Iterates `plan.resources`.
   - Returns `Err(MapperError::EmptyTargetType { ... })` if any resource has
     `target_type.is_empty()`.
   - Returns `Err(MapperError::UnsupportedTargetType { ... })` if any
     resource has a `target_type` not in `supported_types`.
2. Add a private helper `supported_types_for(target_provider)` that returns
   the `Vec<String>` of `TemplateRegistry.stage1().keys()` filtered to those
   starting with `<target_provider>_`. Currently the registry doesn't expose
   `keys()` publicly — small additive change to `templates.rs` (NEXT bullet).
3. `KnowledgeService::fetch_schema(target_provider, version)` is called
   to populate the per-target-type required-attributes map. Wraps the
   `KnowledgeError` in a new `MapperError::Knowledge(Box<KnowledgeError>)`.
4. Cache invalidation discipline preserved: the SYSTEM_PROMPT version bump
   makes the v1 cache cold; this is documented in the commit message.

### `libs/engine/src/generator/templates.rs` ([MODIFY], +5)

Add a public method on `TemplateRegistry`:

```rust
impl TemplateRegistry {
    /// Iterate over registered target_types. Used by the Mapper to construct
    /// the VALID TARGET TYPES prompt block.
    pub fn registered_types(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.by_type.keys().copied()
    }
}
```

Pure additive; no behavior change for existing callers.

### `cli/src/commands/migrate.rs` ([MODIFY], +30)

1. After resolving `--source` and `--to`, compute the default-output path
   per Q3 of clarify.md:

   ```rust
   fn default_output_for(source: &Path, target_provider: &str) -> PathBuf {
       let stem = source.file_name().and_then(|s| s.to_str());
       match stem {
           Some(s) if !s.is_empty() && s != "." && s != ".." => {
               source.with_file_name(format!("{s}-{target_provider}"))
           }
           _ => PathBuf::from(format!("./terrashift-output-{target_provider}")),
       }
   }
   ```

2. Replace the existing tempdir fallback with `default_output_for(source, &args.to_provider)`.
3. Print the chosen default path explicitly so the user sees where it landed:
   `📁 No --output specified; defaulting to <path>`.

### Tests

#### Unit (Mapper)

- `libs/engine/src/mapper/mod.rs` `#[cfg(test)] mod tests`:
  - `mapper_rejects_empty_target_type` — feed `StubClient` a response with
    `target_type: ""`, assert `Err(MapperError::EmptyTargetType { source_addr })`.
  - `mapper_rejects_unsupported_target_type` — feed `target_type: "azurerm_virtual_machine"`
    (deprecated, not in registry), assert `Err(MapperError::UnsupportedTargetType { ... })`.
  - `mapper_accepts_valid_target_types` — feed all 5 supported types, assert `Ok(plan)`.
  - `mapper_prompt_includes_required_attributes_when_schema_cached` — pre-populate
    schema cache with `azurerm_virtual_network { required: ["name", ...] }`, build
    prompt, assert `prompt.contains("MUST set: name")`.
  - `mapper_prompt_falls_back_when_schema_cache_empty` — empty cache, build prompt,
    assert no panic + prompt does NOT contain `REQUIRED ATTRIBUTES` header.

#### Unit (default-output)

- `cli/src/commands/migrate.rs` `#[cfg(test)] mod tests`:
  - `default_output_for_normal_path` — `./aws-app + azurerm` → `./aws-app-azurerm`.
  - `default_output_for_trailing_slash` — `./aws-app/ + azurerm` → `./aws-app-azurerm`.
  - `default_output_for_dot` — `. + azurerm` → `./terrashift-output-azurerm`.
  - `default_output_for_dotdot` — `.. + azurerm` → `./terrashift-output-azurerm`.
  - `default_output_for_absolute` — `/home/x/aws-app + azurerm` → `/home/x/aws-app-azurerm`.

#### E2E (manual, gated)

- Run `fixtures/e2e-aws-to-azure/` against rebuilt binary with `HF_TOKEN` set.
- Assert ≥4 of 5 resources emit (via the migration summary "HCL files emitted: ≥4").
- Document this as a manual step in the PR description; not in CI yet (HF_TOKEN
  is operator-supplied).

## Test plan ordering

1. Run unit tests first (fast feedback): `cargo test -p terrashift-engine --lib`.
2. Run workspace tests: `cargo test --workspace`.
3. Build the binary: `cargo build -p terrashift`.
4. Run e2e fixture manually with `HF_TOKEN`.

## Backward compatibility notes

- **Existing cached Mapper outputs become cold.** Article XII rule 2 expected.
- **Existing CLI invocations with `--output <path>`** are unchanged.
- **Existing CLI invocations without `--output`** stop emitting to a tempdir
  and start emitting to `<source-stem>-<target>/`. This is a behavioral change
  but in the user's favor (they keep their files). PR description notes it
  explicitly.
- **No new workspace dependencies.** All work uses existing crates.

## Observability

- New `tracing::info!` lines for: schema-cache hit-or-miss in prompt build,
  default-output path resolved, validation errors before they propagate.
- Error variants carry the source resource address so users can find the
  offending resource in their tree.

## Order of file changes during implementation

1. `templates.rs` — add `registered_types()` (smallest, no consumers depend on it yet).
2. `errors.rs` — add the three new variants (compile-only, no runtime path).
3. `prompt.rs` — refactor `SYSTEM_PROMPT` into header-const + builder; bump version string.
4. `mod.rs` — wire validation pass + schema lookup; add unit tests.
5. `migrate.rs` — default-output computation; add unit tests.
6. `cargo fmt` + `cargo clippy --workspace --all-targets` clean before commit.
7. `cargo test --workspace`.
8. Build binary; run e2e fixture manually.
