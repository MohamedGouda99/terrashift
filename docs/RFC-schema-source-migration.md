# RFC — Schema source migration

**Status:** Draft, awaiting founder review.
**Author:** Principal infrastructure engineer (contractor).
**Phase:** 3.3 (study + design). **Phase 4 implementation does not begin until founder confirms or 24 hours elapse, whichever comes first.**
**Date:** 2026-05-07

---

## 0. Summary

Establish `terraform providers schema -json` (via the already-implemented `TerraformCliSchemaFetcher` at `libs/knowledge/src/schema_fetcher_cli.rs:53-258`) as Terrashift's authoritative schema source. Build out an end-to-end production path: a versioned on-disk cache at `~/.terrashift/schemas/`, a build-time bundled-schema xtask that ships pre-extracted schemas in the release binary, five new `terrashift schema {list,update,show,verify,gc}` CLI subcommands, an audit-logged auto-update flow, an Ed25519-signed `AuditPayload::SchemaCapture` variant, and an extension to `GoldenManifest` so eval fixtures declare their required schemas. The `TerraformRegistryClient` HTTP client is **retained** for version-metadata listing only — `terrashift schema update` needs it to answer "what versions can I pin to?" before the user picks a constraint. The categorised `libs/knowledge/seed/{provider}/{category}/{resource}.json` layout (CloudForge-derived) is **deleted entirely** and replaced with `libs/knowledge/seed/{provider}/{version}/schema.json`. P-07 is rewritten — its current claim that the Knowledge service "calls registry.terraform.io and populates cache" (`docs/development/terrashift_prompts.md:444`) is the exact sentence being purged.

---

## 1. Findings (one paragraph per crate touched)

### `libs/knowledge` — the schema source-of-truth crate

The crate is structurally complete for this migration. `SchemaStore` at `schema_store.rs:21-44` is a 4-method async trait (`fetch_provider_schema`, `cache_schema`, `list_versions`, `search_mappings`) with one impl, `LocalSchemaStore` (`local_schema_store.rs:34-36`) — a sqlx + SQLite store with a single-table DDL at `local_schema_store.rs:22-31` (`provider TEXT, version TEXT, schema_json TEXT, fetched_at TEXT, PRIMARY KEY (provider, version)`). `TerraformRegistryClient` at `registry_client.rs:92-95` exposes only metadata-listing methods (`get_provider`, `list_versions`); the module docstring at `registry_client.rs:14-20` correctly states "The Registry API itself doesn't return schemas — they live inside the provider binary." `TerraformCliSchemaFetcher` at `schema_fetcher_cli.rs:53-258` is a complete, unconditionally-compiled, non-feature-gated impl of the `SchemaFetcher` trait. It runs the canonical `tempdir → versions.tf → terraform init -input=false -no-color → terraform providers schema -json → ProviderSchema` flow, parses the heterogeneous Terraform type JSON via `render_type` (`schema_fetcher_cli.rs:353-393`), and surfaces `RegistryError::TerraformUnavailable` from `check_available()` at `schema_fetcher_cli.rs:93-113`. `ProviderSchema`, `ResourceSchema`, and `AttributeSchema` are defined at `types.rs:17-49`. `KnowledgeService` at `knowledge_service.rs:74-79` is the orchestrator with `sync_provider`, `seed_provider`, `seed_from_bundle`, `first_launch_sync`, `find_similar_resources`, and `fetch_schema` (the only method Validator calls). The seed bundle at `libs/knowledge/seed/` ships ~170 hand-curated `ResourceSchema` JSON files in `{provider}/{category}/{resource}.json` shape, produced by `scripts/import-resource-catalog.mjs` from a CloudForge TypeScript catalog.

### `libs/engine/src/validator` — the schema consumer

The Validator has exactly one schema fetch site: `validator/mod.rs:87-89` calls `self.knowledge.fetch_schema(&plan.target_provider, target_version).await?`. Subsequent reads check resource-type existence (`mod.rs:96`), per-attribute existence (`mod.rs:109`), deprecation flag (`mod.rs:116`), computed-but-not-optional (`mod.rs:128`), and required-but-missing (`mod.rs:140-142`). Three blocking error variants live in `validator/report.rs:47-67`: `UnknownResourceType`, `UnknownAttribute` (both cite "Article III violation" in display), and `MissingRequiredAttribute` (cites "Article IV violation"). Two non-blocking warning variants live at `validator/report.rs:91-108`. The infrastructure error path is `ValidatorError::Knowledge(KnowledgeError)` at `validator/errors.rs:16-22` — fired only when `fetch_schema` itself fails (cache miss, storage failure). Per-resource findings never surface as `Err`. Seven non-ignored tests live at `validator_test.rs` covering valid plan, hallucinated attribute, missing required, unknown resource, deprecated warning, multi-error aggregation, and computed-attribute warning. The schema-consumer surface is stable; this RFC does not change it.

### `libs/audit` — where the `SchemaCapture` variant lands

`AuditEntry` at `entry.rs:19-32` carries a hash chain (`prev_hash`, `content_hash`, `signature`). `AuditPayload` at `entry.rs:62-107` is an internally-tagged enum (`#[serde(tag = "type", rename_all = "snake_case")]`) with five existing variants: `LlmCall`, `ToolExecution`, `PhaseTransition`, `CredentialResolution`, `FileOperation`. The new `SchemaCapture` variant must follow the named-fields-only, snake-case-discriminator pattern. SHA-256 hash chain is computed in `compute_content_hash` at `store.rs:269-290` over a canonical JSON tuple of `(id, timestamp.to_rfc3339(), run_id, &actor, &operation, &outcome, &payload, &prev_hash)`. Ed25519 signing happens at `store.rs:165` (`entry.signature = signer.sign(&entry.content_hash);`). The non-bypassable scrubber `scrub_or_panic` runs at `store.rs:152` before any hash or DB write — it scans every string field of every payload variant for the patterns at `scrubber.rs:38-58` (AWS access key, GCP API key, GitHub PAT, private-key header, plus a high-entropy heuristic) and panics on match. The scrub match arms at `store.rs:311-355` enumerate every variant by name; the new `SchemaCapture` arm must be added there too. The only impl of `AuditStore` is `LocalAuditStore` (`store.rs:75-81`); the only two production write sites today are `AuditWriterHook::after_tool_execution` at `hooks.rs:59-64` (deferred wiring per `hooks.rs:11`) and `StubBroker::audit_fetch` at `libs/creds/src/stub.rs:85-96`.

### `libs/creds` — peripheral; no PATH search lives here

The crate contains **no external binary invocations and no PATH search code**. No `Command::new`, no `which`-crate use, no `std::process` imports anywhere in `libs/creds/src/**`. The credential brokers (`AwsBroker`, `GcpBroker`, `AzureBroker`) all return `NotImplementedYet` until S5. The `StubBroker` is the only one that actually runs in production paths, and it operates entirely in-memory. The Article-V relevant pieces are: `Credential` at `broker.rs:26-61` (wraps raw value in `Zeroizing<String>`, `Debug` impl always emits `<redacted>`), `pre_llm_check` at `scrub.rs:23-41` (delegates to `terrashift_audit::scrubber::scan` and returns `CredsError::SecretsDetected`), and `SubstitutionMap::rebuild` at `substitution.rs:52-60` (reverse-substitutes resolved values back to `{{secret:NAME}}` placeholders so audit emission carries references, never raw values). For this RFC, `libs/creds` is not modified — the schema-capture path needs `terraform` on PATH but does not handle credentials, and PATH resolution lives where it belongs at `libs/knowledge/src/schema_fetcher_cli.rs:53-65`.

### `libs/shared` — does NOT contain `config.rs` (see Surprise S-1)

`libs/shared/src/config.rs` **does not exist**. The crate is currently a stub; `libs/shared/src/lib.rs` contains only a `crate_compiles()` test (`shared/src/lib.rs:26`). The prompt's section 4.3 directs the contractor to add `[profiles.<name>.schemas]` to `libs/shared/src/config.rs` — that file is fictional. The actual `Profile` struct is at `libs/ai/src/profile.rs`. This RFC's recommended default (see OQ-1) is to extend `libs/ai/src/profile.rs` rather than relocate `Profile` to `libs/shared` as a prerequisite refactor.

### `libs/ai` — where `Profile` actually lives

`Profile` at `libs/ai/src/profile.rs` declares `model: Option<String>`, `tiers: Tiers`, and `providers: BTreeMap<String, ProviderConfig>`. `Tiers` has `eco: String` (required) and `smart: Option<String>` (falls back to `eco` via `model_for_tier` at `profile.rs:94-103`). Per-provider nested config is the precedent for the new `[profiles.<name>.schemas]` section: `[profiles.default.providers.huggingface]` is the analogous existing pattern. `Profile::from_toml` at `profile.rs:78-89` tries `ProfileBundle` first then bare `Profile`; errors box `toml::de::Error` into `AiError::TomlParse`. The CLI loads via `cli/src/commands/util.rs:44-52` (`load_profile`).

### `cli` — the CLI surface

`cli/src/main.rs:27-51` declares `Cli` (with `--profile` as `global = true`) and a `Command` enum: `Version`, `Migrate(commands::migrate::Args)`, `Schemas(commands::schemas::Cmd)` (already a subcommand-of-subcommand), `Scan(commands::scan::Args)`. **`commands::schemas` already exists as a single file** at `cli/src/commands/schemas.rs` — not a directory. Its current `Cmd` enum at `schemas.rs:17-46` has three variants: `Sync` (the only place `TerraformCliSchemaFetcher` is wired in production today, at `schemas.rs:91-95`), `List` (hardcodes 4 provider names at `schemas.rs:128`), and `Seed` (uses `StubSchemaFetcher` + in-memory store via `super::util::build_knowledge_service`). The new five subcommands (`list`, `update`, `show`, `verify`, `gc`) will replace `Sync`/`List`/`Seed` — `update` subsumes `Sync`'s function; `list` replaces it with dynamic provider enumeration; `Seed` is deleted (its purpose was an offline smoke test that the new path makes redundant). `commands::util` at `cli/src/commands/util.rs` provides `VECTOR_DIM = 384`, `resolve_profile_path`, `home_dir` (USERPROFILE-then-HOME priority), `load_profile`, `locate_seed_dir` (5-candidate fallback chain), `build_knowledge_service`, `build_llm_client`. The clap idiom in this codebase: `#[derive(clap::Args, Debug)]` for flat structs, `#[derive(clap::Subcommand, Debug)]` for enums, `///` doc comments as the only help-text source, `default_value_t = false` for booleans, no `help = "..."` attributes, `default_value = "literal"` for string defaults. Subprocess-driven smoke tests live at `cli/tests/cli_smoke_test.rs` (9 tests, all use `CARGO_BIN_EXE_terrashift`).

### `tui` — the slash command surface

`tui/` is a top-level workspace crate (`tui/Cargo.toml`), not a subdirectory of `cli`. `start_tui` at `event_loop.rs:38-75` initialises the alternate screen, registers a `TerminalGuard` RAII guard, builds `Registry::stage1()`, constructs `AppState::new(status)`, and enters the event loop. `StatusInfo` at `app.rs:47-52` carries `profile_path`, `seed_resources: Option<usize>` (always `None` at launch — the caller in `main.rs:99` constructs it that way), and `tier`. The slash-command system at `tui/src/commands/mod.rs` defines `SlashCommand` trait (sync `fn run(...) -> CommandOutcome`), `CommandContext`, and a `Registry` built via explicit `BTreeMap` insertions in `Registry::stage1()` — 11 entries total (with `quit` and `exit` both mapped to `Box::new(quit::Quit)`). Each command is a unit struct with a manual `impl SlashCommand`. Adding a new `/schemas` command requires three things per `mod.rs:13`: (1) new file in `tui/src/commands/schemas.rs`, (2) `pub mod schemas;` in `mod.rs`, (3) one `commands.insert("schemas", Box::new(schemas::Schemas))` line in `Registry::stage1()`. **`SlashCommand::run` is synchronous** (`mod.rs:44`) — see OQ-2. Existing tests at `tui/tests/{app_state,slash_commands,view_render}_test.rs` cover ~36 cases of state mutation, dispatch, rendering edge cases.

### `libs/eval` — the eval framework

`GoldenManifest` at `golden.rs:25-40` is the load-bearing struct: 6 fields (`name: String`, `description: String` with `#[serde(default)]`, `source_provider: String`, `target_provider: String`, `token_cost_ceiling_micros: u64` with default, `articles: Vec<u8>` with default). Derives only `Deserialize` (no `Serialize` — see OQ-4). `GoldenMigration` at `golden.rs:43-49` aggregates manifest + paths + parsed `MappingPlan`. `load_golden` at `golden.rs:55-124` runs an 8-step validation chain firing `EvalError::MissingFixtureFile { path: PathBuf, missing: &'static str }` (note the `'static str`) for absent files, plus typed parse-error variants. `discover_suite` at `golden.rs:130-151` walks one level deep, skips dotfiles, sorts lexicographically. `EvalRunner::run` at `runner.rs:70-102` is a 5-step flow (Instant → 2× TempDir → Generator → `compare_directories` → result) with `token_cost_micros: 0` hardcoded for Stage 1. `compare_directories` at `scorer.rs:127-133` is strict byte equality on `.tf`-extension files only — schema-cache files (`.json.zst`) will never be touched by the scorer. `Baseline` at `baseline.rs` is currently all-zeros across all 13 fixtures (`eval-baseline.json` lines 1-19); the regression gate is therefore a no-op until S4 lands real Mapper costs. `compare_against_baseline` at `baseline.rs:161-220` produces `Pass`/`Fail` verdicts with `Lenient`/`Strict` modes; `delta_pct > 30.0` triggers `Fail`.

### `terrashift-evals` — the golden corpus

13 fixture directories (`001` through `012`, plus `015` — fixtures 013 and 014 do not exist). Each directory has the shape `manifest.toml` + `source.tf` + `mapping_plan.json` + `expected/<target_type>.tf`. **The directory names encode the target type, not the source.** `003_aws_s3_bucket/manifest.toml` declares `source_provider = "google"`, `target_provider = "aws"` — it's a GCP→AWS migration, not AWS-to-AWS. Same for `012_gcp_storage_bucket_lifecycle` (AWS→GCP). Direction distribution: 7 GCP→AWS (001, 002, 003, 006, 007, 011, 015), 5 AWS→Azure (004, 005, 008, 009, 010), 1 AWS→GCP (012). `terrashift-evals/eval-baseline.json` is the canonical baseline file. The corpus is **fixture data, not a Cargo package** — `cargo nextest run -p terrashift-eval` runs the framework crate against this corpus, not against a package named `terrashift-evals`.

### `Cargo.toml` workspace + per-crate manifests

16 workspace members listed at `Cargo.toml:7-24`. **No `xtask/` crate exists** — there is no workspace member named `xtask` and no `.cargo/config.toml`. Workspace dependencies at `Cargo.toml:39-115` include the full set tracked through this RFC: `tokio`, `serde`, `serde_json`, `toml`, `thiserror`, `anyhow`, `tracing`, `reqwest` (rustls-only, per `Cargo.toml:60`), `ratatui`, `clap`, `sqlx` (with sqlite, macros, chrono, uuid features), `chrono`, `uuid`, `ed25519-dalek`, `sha2`, `similar`, `tempfile`, `tree-sitter`. **`which`, `zstd`, `memmap2`, `dashmap`, and `humansize` are absent** from both workspace-level and per-crate dependencies. `lancedb` at `Cargo.toml:85` is a dead workspace dependency entry — the only crate that would use it has it commented out (`libs/knowledge/Cargo.toml:26`). Workspace lints at `Cargo.toml:135-139` (`unwrap_used = "deny"`, `expect_used = "deny"`, `string_slice = "deny"`) apply to every new file. CI at `.github/workflows/ci.yml` has 4 jobs (`fmt`, `clippy`, `test`, `eval`); test matrix is `[ubuntu-latest, macos-latest]` — Windows is absent (see OQ-6).

---

## 2. Where the Registry API path lives today

Every reference to `registry.terraform.io` in the entire workspace, classified by load-bearing-ness:

| File:line | Kind | Disposition |
|---|---|---|
| `libs/knowledge/src/registry_client.rs:13` | Module docstring | KEEP — accurate explanation |
| `libs/knowledge/src/registry_client.rs:14-20` | Module docstring (the "schemas live in the binary, only readable via subcommand" paragraph) | KEEP verbatim — it's the canonical explanation |
| `libs/knowledge/src/registry_client.rs:23-30` | **Stale** docstring claiming `TerraformCliSchemaFetcher` "arrives in S4" | REWRITE — the impl is already in `schema_fetcher_cli.rs:53-258` |
| `libs/knowledge/src/registry_client.rs:40` | `const REGISTRY_BASE: &str = "https://registry.terraform.io/v1/providers"` | KEEP — used by the metadata-listing methods we retain |
| `libs/knowledge/src/registry_client.rs:116-153` | `get_provider`, `list_versions` HTTP methods | KEEP — these are the metadata-listing path the new `terrashift schema update` CLI needs |
| `libs/knowledge/src/registry_client.rs:163-217` | `SchemaFetcher` trait + `StubSchemaFetcher` impl | KEEP — the trait is the right abstraction; stub stays for tests |
| `libs/knowledge/src/schema_fetcher_cli.rs:222` | The string `"registry.terraform.io/{namespace}/{name}"` used as a JSON-parse key | KEEP — this is parsing terraform's own output format, not an HTTP call |
| `libs/knowledge/seed/README.md:6` | Docstring explaining seed bundle | DELETE (entire seed README is rewritten — see §3 deletes) |
| `README.md:172` | Top-level docstring | UPDATE — point at new schema model |
| `docs/architecture/terrashift_plan.md:201` | Plan reference | UPDATE in §6 doc-update phase |
| `docs/development/terrashift_prompts.md:444` | **The exact load-bearing sentence** "Hits cache first; on miss, calls registry.terraform.io and populates cache." | REWRITE — this is the wrong claim the RFC purges |
| `docs/governance/SESSION_PLAN.md:139` | Session-plan reference | UPDATE in §6 doc-update phase |
| `specs/007-knowledge-schema-cache/spec.md:11,27` | Spec doc | UPDATE in §6 doc-update phase |
| `cli/src/commands/schemas.rs:21` | Doc comment for `Cmd::Sync` ("Pull provider schemas from the Terraform Registry") | REWRITE — pulls from terraform CLI subprocess, not the registry |

**Net schema-fetch-claim-vs-reality:** zero current code paths fetch schemas via HTTP from the registry. The only places that *claim* this are docstrings and the P-07 prompt at line 444. The `TerraformCliSchemaFetcher` is wired (in `cli/src/commands/schemas.rs:91-95` for `Sync`, in tests via `StubSchemaFetcher`) but has no production CLI surface beyond `terrashift schemas sync`. The migration's *real work* is wiring it to a production CLI surface and adding the cache + manifest + audit trail.

---

## 3. Concrete change list

### `libs/knowledge`

- `[NEW]` `libs/knowledge/src/manifest.rs` — `RuntimeManifest` + `RuntimeManifestEntry` types matching the prompt's JSON schema in section 4.4 (`schema_version: u32`, `schemas: Vec<...>`). Implements load/save with atomic-rename pattern. `serde_json` only — no free-form parsing.
- `[NEW]` `libs/knowledge/src/manifest/migrations.rs` — schema_version-bumping migration scaffold per prompt section 4.4 line 235. Empty for v1; in place to make future bumps a one-liner.
- `[NEW]` `libs/knowledge/src/cache.rs` — `RuntimeSchemaCache` struct that owns the manifest + `tokio::sync::OnceCell<Arc<ProviderSchema>>` per `(provider, version)`. Hot-path readers go through this cache only; first-touch reads via `Arc::new(serde_json::from_slice(&fs::read(path)?)?)` then `OnceCell::set`. **No `memmap2`** — `serde_json` parses every byte on first touch regardless of source.
- `[MODIFY]` `libs/knowledge/src/schema_store.rs` — extend `SchemaStore` with `list_providers() -> Result<Vec<String>>`. Replaces the hardcoded provider list at `cli/src/commands/schemas.rs:128`.
- `[MODIFY]` `libs/knowledge/src/local_schema_store.rs` — implement `list_providers()` via `SELECT DISTINCT provider FROM provider_schemas`.
- `[MODIFY]` `libs/knowledge/src/registry_client.rs` — delete the stale "S4 arrival" docstring at lines 23-30. Add a new method `pub async fn list_versions_matching(&self, namespace, name, constraint: &VersionReq) -> Result<Vec<String>>` (semver-aware, used by `terrashift schema update`). KEEP `get_provider`, `list_versions`, `with_base_url`, all metadata types.
- `[MODIFY]` `libs/knowledge/src/knowledge_service.rs` — replace `seed_from_bundle` (currently walks `{provider}/{category}/{resource}.json`) with a new `seed_from_runtime_cache(cache_root: &Path)` that walks `{provider}/{version}/schema.json` per the new layout in prompt section 4.4. Delete the `terrashift-seed-2025.01` constant at `knowledge_service.rs:517`.
- `[MODIFY]` `libs/knowledge/src/lib.rs` — re-export new `manifest` and `cache` modules.
- `[DELETE]` `libs/knowledge/seed/{aws,azurerm,google}/{networking,compute,database,...}/*.json` — every category subdirectory enumerated in OQ-3. The new flat-per-version layout from prompt section 4.4 replaces them.
- `[DELETE]` `libs/knowledge/seed/scripts/import-resource-catalog.mjs` — CloudForge importer; obsoleted by `cargo xtask capture-schemas`.
- `[DELETE]` `libs/knowledge/seed/README.md` (current version) and `[NEW]` rewritten version describing the flat-per-version layout, the bundled-extract behaviour, and `terrashift schema update`.
- `[MODIFY]` `libs/knowledge/tests/seed_bundle_integrity_test.rs` — rewrite all 5 tests to assert the new flat-per-version layout. Discipline preserved: every JSON parses, idempotency on rerun, provider filter correctness. New invariant: every `(provider, version)` directory matches a manifest entry.
- `[MODIFY]` `libs/knowledge/tests/first_launch_sync_test.rs` — point fixture paths at the new layout. Invariants unchanged.
- `[MODIFY]` `libs/knowledge/tests/schema_cache_test.rs` — add tests for new `list_providers()` method and the deduplication behaviour of `list_versions_matching`.
- `[MODIFY]` `libs/knowledge/Cargo.toml` — add `which = "6"` (binary discovery), `dashmap = "6"` (cache hot-path) and remove the commented-out `lancedb` line.

### `libs/audit`

- `[MODIFY]` `libs/audit/src/entry.rs` — add `SchemaCapture` variant to `AuditPayload` per prompt section 4.7. Fields: `provider: String`, `source: String`, `version_constraint: String`, `resolved_version: String`, `terraform_version: String`, `captured_via: CaptureVia` (new enum), `sha256: String`, `duration_ms: u32`. Plus add `CaptureVia` enum (`Bundled`, `UserCommand`, `AutoUpdate`).
- `[MODIFY]` `libs/audit/src/store.rs` — add `SchemaCapture` arm to the scrub-or-panic match (`store.rs:311-355`). Scan `provider`, `source`, `version_constraint`, `resolved_version`, `terraform_version`, `sha256` for the standard secret patterns. The `sha256` field is high-entropy by definition; add an exemption case in the heuristic for hex-only strings of length 64 (SHA-256 sized).
- `[MODIFY]` `libs/audit/tests/audit_chain_test.rs` — add `schema_capture_payload_records_provider_version_terraform` and `tampering_with_resolved_version_breaks_chain` tests mirroring the existing `LlmCall` and content-tampering tests.
- `[MODIFY]` `libs/audit/src/scrubber.rs` — exempt 64-char lowercase-hex strings from the high-entropy heuristic (matches SHA-256 hex output).
- `[MODIFY]` `libs/audit/src/scrubber.rs` — fix the `PATTERN_NAMES` mismatch (Surprise S-2): `PATTERN_NAMES` has 5 entries but `patterns()` builds 4 — remove `"high_entropy_token"` from the constant and document the heuristic separately.

### `libs/ai`

- `[MODIFY]` `libs/ai/src/profile.rs` — extend `Profile` with a `schemas: SchemasConfig` field, `#[serde(default)]`. Add `SchemasConfig` struct with `pinned: Vec<PinnedSchema>` (with `#[serde(default)]`), `auto_update: AutoUpdateCadence` (enum: `Off | Weekly | Monthly`, default `Off`), `auto_update_window: AutoUpdateWindow` (enum: `OffHours | Anytime`, default `OffHours`). Also add `PinnedSchema { provider: String, version: String }`.
- `[MODIFY]` `libs/ai/src/lib.rs` — re-export `SchemasConfig`, `PinnedSchema`, `AutoUpdateCadence`.
- `[MODIFY]` `assets/profile.example.toml` — add a documented `[profiles.default.schemas]` block matching prompt section 4.3.

### `libs/eval`

- `[MODIFY]` `libs/eval/src/golden.rs` — extend `GoldenManifest` with `required_schemas: Vec<RequiredSchema>` field, `#[serde(default)]` (matching the existing pattern for `description`, `token_cost_ceiling_micros`, `articles`). Add `RequiredSchema { provider: String, version: String }`. Per prompt section 5.1: do NOT introduce a parallel reader — extend the existing struct.
- `[MODIFY]` `libs/eval/src/runner.rs` — insert a schema-presence check in `EvalRunner::run` between TempDir creation (step 2) and Generator invocation (step 3). Each `RequiredSchema` is verified present in `terrashift-evals/.schema-cache/`.
- `[MODIFY]` `libs/eval/src/errors.rs` — add `MissingSchema { fixture: PathBuf, schema_id: String }` variant (per OQ-5; reusing `MissingFixtureFile` cannot work because `missing` is `&'static str`).
- `[MODIFY]` `libs/eval/Cargo.toml` — add `zstd = "0.13"` for decompressing the eval schema cache.

### `cli`

- `[MODIFY]` `cli/src/main.rs` — change `Schemas(commands::schemas::Cmd)` (subcommand) to `Schema(commands::schema::Cmd)` (singular, matching the prompt's CLI surface). Add `Schema` to dispatch at `main.rs:79`. Pass `cli.profile` to `commands::schema::run` (closes Surprise S-3).
- `[NEW]` `cli/src/commands/schema/mod.rs` — top-level `Cmd` enum with five variants: `List`, `Update`, `Show`, `Verify`, `Gc`. Each is a separate sub-module file.
- `[NEW]` `cli/src/commands/schema/list.rs` — `Args` struct + `run()`. Reads the runtime manifest, formats columns: `provider · version · captured · size · sha256 prefix`. TTY-aware output (no ANSI when not a TTY, per prompt section 4.2).
- `[NEW]` `cli/src/commands/schema/update.rs` — `Args { provider, version, all, non_interactive }`. Resolves available versions via `TerraformRegistryClient::list_versions_matching` for constraint expansion, then calls `TerraformCliSchemaFetcher::fetch` per `(provider, version)`, writes `schema.json` + `capture.log`, updates manifest, emits `AuditPayload::SchemaCapture` per provider. 60s per-provider timeout (matches `TerraformCliSchemaFetcher::with_timeout`). Surfaces `RegistryError::TerraformUnavailable` with platform-aware install hint per prompt section 4.2.
- `[NEW]` `cli/src/commands/schema/show.rs` — `Args { spec: String, filter: Option<String> }`. Parses `provider@version`, loads schema via cache, prints alphabetically-sorted resource list with optional prefix filter. Pipe-friendly when not TTY.
- `[NEW]` `cli/src/commands/schema/verify.rs` — `Args { profile_only: bool }`. Walks every cached `(provider, version)`, recomputes SHA-256, compares with manifest, exits 1 on any drift. Produces `error: schema sha256 mismatch for aws@5.30.0` lines.
- `[NEW]` `cli/src/commands/schema/gc.rs` — `Args { keep_versions: u32 }`. Deletes `(provider, version)` cache directories not referenced by any profile pin and beyond `keep_versions` (default 2).
- `[NEW]` `cli/src/commands/schema/install_hint.rs` — platform-detection helper that emits `brew install terraform` (macOS), `sudo apt install terraform` (Debian/Ubuntu), `choco install terraform` (Windows), plus the terraform.io download URL.
- `[DELETE]` `cli/src/commands/schemas.rs` — entire single-file module. Replaced by the new directory above.
- `[MODIFY]` `cli/src/commands/mod.rs` — replace `pub mod schemas;` with `pub mod schema;`.
- `[MODIFY]` `cli/src/commands/util.rs` — replace `build_knowledge_service` (currently uses `StubSchemaFetcher`) with one that wires `TerraformCliSchemaFetcher` for `--profile`-driven invocations and `StubSchemaFetcher` only when no profile is loaded (eg, smoke tests).
- `[MODIFY]` `cli/tests/cli_smoke_test.rs` — extend smoke tests with: `schema_list_with_no_cache_succeeds`, `schema_update_without_terraform_emits_install_hint_not_panic`, `schema_show_pipe_emits_no_ansi`, `schema_verify_corrupt_file_exits_1`, `schema_gc_dry_run_does_not_delete`. Replace existing `schemas_list_with_no_cache_succeeds` and `schemas_help_lists_subcommands` (they reference the deleted singular subcommand layout).

### `tui`

- `[NEW]` `tui/src/commands/schemas.rs` — `pub struct Schemas;` with manual `impl SlashCommand`. Per OQ-2, this is a thin shim that emits `CommandOutcome::Hint("To manage cached schemas, run `terrashift schema list`. The CLI is the canonical surface; the TUI shows current cache state in the status footer.")`. This matches the existing `/migrate` and `/scan` patterns at `tui/src/commands/{migrate,scan}.rs`.
- `[MODIFY]` `tui/src/commands/mod.rs` — add `pub mod schemas;` and `commands.insert("schemas", Box::new(schemas::Schemas))`.
- `[MODIFY]` `tui/src/app.rs` — extend `StatusInfo` with `cached_schema_count: Option<usize>` (analogous to `seed_resources`). Status footer renders as e.g. `5 schemas cached` or `not loaded`.
- `[MODIFY]` `tui/src/view.rs` — render the new status field in the footer at `view.rs:141-191`.
- `[MODIFY]` `cli/src/main.rs:90-107` (`launch_tui`) — populate `cached_schema_count` from manifest at TUI launch (synchronous read; manifest is small).
- `[MODIFY]` `tui/tests/slash_commands_test.rs` — add `schemas_emits_hint_pointing_at_cli`. Update `registry_has_eleven_commands` to `twelve_commands` (or whatever the new count is).
- `[MODIFY]` `tui/tests/view_render_test.rs` — add a render test asserting `cached_schema_count` shows up in the status footer.

### `terrashift-evals`

- `[NEW]` `terrashift-evals/.schema-cache/` directory (directory itself; will be checked into git).
- `[NEW]` `terrashift-evals/.schema-cache/<provider>@<version>.json.zst` — one compressed schema per `(provider, version)` referenced by any fixture's `required_schemas`. zstd level 19 (per prompt section 5.2).
- `[NEW]` `terrashift-evals/.schema-cache/manifest.json` — uncompressed sidecar listing every captured schema with `sha256`, `terraform_version`, `captured_at`. PR reviewers can spot drift in the manifest without decompressing.
- `[NEW]` `terrashift-evals/.gitattributes` — `*.json.zst binary` — prevents git from attempting to render diffs on the compressed blobs.
- `[MODIFY]` Each of the 13 fixture `manifest.toml` files (001 through 012, 015) — add `required_schemas = [...]` based on the source/target provider pair. Eg `001_aws_vpc_minimal/manifest.toml` adds `required_schemas = [{ provider = "aws", version = "~> 5.30" }]`.

### `xtask` (new workspace member)

- `[NEW]` `xtask/Cargo.toml` — new package, depends on `serde_json`, `tokio`, `tempfile`, `which`, `zstd`, `terrashift-knowledge` (for `TerraformCliSchemaFetcher`), `terrashift-eval` (for `discover_suite`).
- `[NEW]` `xtask/src/main.rs` — clap-driven CLI dispatching to subcommands.
- `[NEW]` `xtask/src/capture_schemas.rs` — reads `libs/knowledge/seed/manifest.toml` (NEW file, see below), runs `terraform init && terraform providers schema -json` per pin, splits output by provider via `serde_json` (NOT `jq`), writes `libs/knowledge/seed/{provider}/{version}/schema.json`. Sets `TF_PLUGIN_CACHE_DIR=target/build-schemas/plugin-cache/` for re-run efficiency.
- `[NEW]` `xtask/src/capture_eval_schemas.rs` — calls `discover_suite` on `terrashift-evals/`, computes union of all `required_schemas`, captures each into `terrashift-evals/.schema-cache/<provider>@<version>.json.zst`. Updates `.schema-cache/manifest.json`.
- `[NEW]` `xtask/src/verify_eval_schemas.rs` — recomputes SHA-256 of every `.schema-cache/*.json.zst`, compares with manifest. Exits 1 on drift. CI Job A in prompt section 5.4.
- `[NEW]` `xtask/src/refresh_eval_schemas.rs` — runs `terraform providers schema -json` for each pinned provider, compares with cached, returns nonzero exit if newer patch available. With `--check` only reports; without `--check` writes new schemas. Used by CI Job C.
- `[NEW]` `libs/knowledge/seed/manifest.toml` — build-time pins file. Initial content:
  ```toml
  [[pin]]
  provider = "aws"
  version = "5.30.0"
  source = "hashicorp/aws"
  
  [[pin]]
  provider = "azurerm"
  version = "3.110.0"
  source = "hashicorp/azurerm"
  
  [[pin]]
  provider = "google"
  version = "5.40.2"
  source = "hashicorp/google"
  ```

### `Cargo.toml` (workspace root)

- `[MODIFY]` `Cargo.toml:7-24` — add `"xtask"` to `members`.
- `[MODIFY]` `Cargo.toml:39-115` — add `which = "6"`, `zstd = "0.13"`, `dashmap = "6"`. Add `terrashift-eval = { path = "libs/eval" }` to internal-crate references (already present at line 124 — verify).
- `[MODIFY]` `Cargo.toml:85` — delete the dead `lancedb` workspace dependency.

### `.cargo/config.toml`

- `[NEW]` — defines `[alias] xtask = "run --package xtask --"` so `cargo xtask <subcommand>` works workspace-wide.

### `.github/workflows/`

- `[MODIFY]` `.github/workflows/ci.yml` — add Job A (`cargo xtask verify-eval-schemas`) before existing `eval` job; existing `eval` job becomes Job B. Add Job C (nightly `cargo xtask refresh-eval-schemas --check`) — new workflow file `nightly-eval-refresh.yml` is cleaner than mixing schedules in `ci.yml`.
- `[NEW]` `.github/workflows/nightly-eval-refresh.yml` — runs daily at 04:00 UTC on `schedule:`, executes `cargo xtask refresh-eval-schemas --check`, opens a PR if newer patches exist.

### Documentation

- `[MODIFY]` `docs/architecture/terrashift_plan.md` §7 — add subsection 7.X "Schema acquisition pipeline" describing the three modes (Bundled, UserCommand, AutoUpdate), the manifest format, the cache layout. Cite Articles III, V, VI, IX, X, XII.
- `[MODIFY]` `docs/development/terrashift_prompts.md:436-458` — rewrite P-07 entirely. Delete the "calls registry.terraform.io and populates cache" sentence (line 444). New text references `cargo xtask capture-schemas` and the bundled-extract path.
- `[MODIFY]` `README.md` — add a "Schema management" section explaining `terrashift schema update`, the bundled cache, the install-hint behaviour.
- `[NEW]` `docs/RFC-schema-source-migration.md` — this document.
- `[NEW]` `docs/UX-pass-2026-05.md` — produced during prompt section 8 work; not in this RFC's deliverables but reserved file.

---

## 4. Surprises and concerns

**S-1 — `libs/shared/src/config.rs` does not exist.** The prompt directs the contractor to "add to `libs/shared/src/config.rs`." There is no such file. `libs/shared/src/lib.rs` is a stub with one test (`shared/src/lib.rs:26`). The actual `Profile` struct is at `libs/ai/src/profile.rs`. Recommended default in OQ-1.

**S-2 — `PATTERN_NAMES` has 5 entries; `patterns()` builds 4 regex objects.** `libs/audit/src/scrubber.rs:23-29` declares 5 pattern names; `patterns()` at `scrubber.rs:38-58` builds 4 compiled regexes; the 5th name (`"high_entropy_token"`) corresponds to an inline heuristic at `scrubber.rs:81-92` with no regex. Any zip of these two arrays goes out of bounds. This is pre-existing; the RFC fixes it as part of the SchemaCapture work because the new variant adds an exempt-hex case to the heuristic anyway.

**S-3 — `commands::schemas::run` does not receive `cli.profile`.** Despite `--profile` being declared `global = true` at `cli/src/main.rs:33`, the `Schemas` arm at `cli/src/main.rs:79` calls `commands::schemas::run(cmd).await?` with no profile argument. Every other subcommand that touches user data gets the profile. The RFC fixes this in the migration to `commands::schema::Cmd`.

**S-4 — `Cmd::List` hardcodes 4 provider names.** `cli/src/commands/schemas.rs:128` iterates a static slice `["aws", "azurerm", "google", "hashicorp"]`. The new `terrashift schema list` will use `SchemaStore::list_providers()` (NEW method) for dynamic enumeration.

**S-5 — `SlashCommand::run` is synchronous; the `/schemas` slash command cannot directly await `LocalSchemaStore` async methods.** Trait at `tui/src/commands/mod.rs:44`. The existing pattern (`/migrate`, `/scan`) is for slash commands to emit a `Hint` pointing at the corresponding shell subcommand. Recommended default in OQ-2.

**S-6 — `TerraformCliSchemaFetcher` is fully implemented but its docstring says "arrives in S4."** `libs/knowledge/src/registry_client.rs:23-30` claims the impl is deferred. The actual impl at `schema_fetcher_cli.rs:53-258` is complete and unconditionally compiled. The RFC fixes the docstring.

**S-7 — Deleting the seed bundle hard-fails 5 integrity tests.** `libs/knowledge/tests/seed_bundle_integrity_test.rs` requires `>= 100` JSON files; `first_launch_sync_test.rs` asserts `seed_resource_count > 0`. The deletion is mandatory; the RFC rewrites the tests against the new flat-per-version layout. Recommended default in OQ-3.

**S-8 — `lancedb` is a dead workspace dependency.** Declared at `Cargo.toml:85` but commented out in the only consumer (`libs/knowledge/Cargo.toml:26`). The RFC removes it as part of the manifest-cleanup pass.

**S-9 — `EvalError::MissingFixtureFile { missing: &'static str }` cannot carry a runtime path.** The new schema-cache-missing error carries a dynamically-constructed string (`.schema-cache/aws@5.30.0.json.zst`). New variant `MissingSchema { fixture: PathBuf, schema_id: String }` is required. Recommended default in OQ-5.

**S-10 — `GoldenManifest` derives only `Deserialize`.** No `Serialize`. The xtask `capture-eval-schemas` reads `required_schemas` but does not write back to manifests, so the missing derive is not blocking now. If `refresh-eval-schemas` later wants to bump versions in manifests, `Serialize` becomes mandatory. Recommended default in OQ-4.

**S-11 — eval-baseline.json has 0 token cost across all 13 fixtures.** Article XII rule 4 regression gate is currently a no-op (`pct_change(0, 0) == 0.0` always passes). Out of scope for this RFC; flagged because the new audit-payload variant carries token-cost adjacent metadata that future Article XII work will read.

**S-12 — Fixtures 013 and 014 do not exist.** The corpus jumps from `012_gcp_storage_bucket_lifecycle` to `015_aws_full_stack`. No code assumes contiguous numbering, so this is not a bug — but the RFC's `required_schemas` extension touches all 13 existing fixtures; future fixtures added in those gaps inherit the same shape automatically.

**S-13 — `AuditWriterHook` is implemented at `libs/audit/src/hooks.rs:59-64` but not wired to any production run.** Comment at `hooks.rs:11` defers wiring to S9. The new `SchemaCapture` audit emission goes through a different path (the new `update.rs` command directly calls `AuditStore::append`), not through `AuditWriterHook`. So this RFC does not depend on the deferred wiring landing first.

**S-14 — `home_dir()` checks `USERPROFILE` before `$HOME`.** `cli/src/commands/util.rs:37-41`. Windows-first ordering — correct for the founder's dev box, but a Linux CI runner with `USERPROFILE` set in the environment (cross-compilation tooling sometimes does this) would resolve to the wrong directory. Pre-existing, out of scope.

**S-15 — CI test matrix excludes Windows.** Only Linux + macOS in `.github/workflows/ci.yml`. The xtask runs on Windows (founder's dev box) but is never tested on Windows in CI. Recommended default in OQ-6.

**S-16 — `StubSchemaFetcher::fetch` ignores the `namespace` argument.** `libs/knowledge/src/registry_client.rs:203,207-209`. Tests passing `"hashicorp"` succeed even if the stub map keys ignore that field. Pre-existing, out of scope. Documented here in case a contractor introduces a non-`hashicorp` namespace in tests and wonders why the stub matches.

---

## 5. Open questions for the founder

Each question below is a real ambiguity that I cannot resolve from code alone. Each has a recommended default that activates after 24 hours of no reply.

**OQ-1 — Where does the `[profiles.<name>.schemas]` config block live, given that `libs/shared/src/config.rs` does not exist?**

**Default if no reply:** Add the new `SchemasConfig` to `libs/ai/src/profile.rs` (where `Profile` is). The crate split between `libs/shared` and `libs/ai` was never materialised for `Profile`, and forcing it now would be an architectural prerequisite that doubles the PR scope. The schemas config logically belongs alongside provider config.

**OQ-2 — How does the new `/schemas` TUI slash command access async `SchemaStore` methods, given `SlashCommand::run` is synchronous?**

**Default if no reply:** `/schemas` emits a `CommandOutcome::Hint` pointing at `terrashift schema list` from the shell. This matches the existing pattern for `/migrate` and `/scan`, neither of which actually runs the heavy work — the TUI is a thin shim, the CLI is canonical. The status footer shows current cache state via the synchronous manifest read at TUI launch.

**OQ-3 — Five tests in `libs/knowledge/tests/seed_bundle_integrity_test.rs` and one assertion in `first_launch_sync_test.rs` will hard-fail after the categorised seed deletion. Rewrite them?**

**Default if no reply:** Yes — rewrite all 6 against the new flat-per-(provider, version) layout. The disciplines (every JSON parses, idempotency on rerun, provider filter correctness, non-empty seed) all remain meaningful and become MORE so against the new layout. New invariant added: every directory matches a manifest entry (catches drift between runtime cache and bundled schemas).

**OQ-4 — Add `Serialize` to `GoldenManifest` now, or wait until the xtask actually needs to write manifests?**

**Default if no reply:** Wait. YAGNI. The xtask `capture-eval-schemas` reads `required_schemas` but does not modify manifests. If `refresh-eval-schemas --apply` (a future, not in scope here) ever needs to write manifests, add `Serialize` then.

**OQ-5 — Add `EvalError::MissingSchema { fixture, schema_id }` variant, or extend `MissingFixtureFile` to carry a `String`?**

**Default if no reply:** Add the new variant. `MissingFixtureFile { missing: &'static str }` was deliberately tightly typed for compile-time-known files (`manifest.toml`, `source.tf`, `mapping_plan.json`, `expected/`). Schema-cache misses are runtime-keyed by `(provider, version)`; a separate variant is the cleaner abstraction.

**OQ-6 — Add Windows to the CI test matrix as part of this PR, or follow-up?**

**Default if no reply:** Follow-up. Adding Windows touches `terraform` install discipline, plugin-cache path semantics, and crossterm rendering — all reasonable but each its own can of worms. The xtask paths will be tested manually on Windows during the section 7.2 integration tests. Open a tracking issue.

**OQ-7 — Compress the eval schema cache as `*.json.zst` (zstd-19) per prompt section 5.2, or check in plain JSON for review-ability?**

**Default if no reply:** Compressed, per the prompt. Plus a sidecar `.schema-cache/manifest.json` (uncompressed) so PR reviewers can spot drift in size, sha256, or terraform_version without decompressing. The `.gitattributes` line forces `binary` so git doesn't render junk diffs on the `.zst` files. The sidecar manifest is the human-readable change-detection artifact.

**OQ-8 — When `auto_update != "off"` and the user invokes a `--non-interactive` migrate, should the background check still fire?**

**Default if no reply:** Yes — the check fires (it's non-blocking) and writes `last_auto_check` to the manifest. The interactive prompt is suppressed in `--non-interactive`, so no schema is actually swapped without user approval. The next interactive command surfaces the prompt. This matches Article VI: pinned schemas are reproducible; the user authorizes every change.

**OQ-9 — `terrashift schema gc` default `--keep-versions = 2` (current + previous) — does this match Article IX's "never auto-delete" stance?**

**Default if no reply:** Yes. Article IX bans deletion of *user data* (state files, audit logs, migration outputs). Cached schema artifacts are not user data — they are reproducibly re-fetchable from `terraform providers schema -json` against the same pin. `gc` defaulting to 2 means the user always has at least one prior version to roll back to (Article XIII rule 2 — monotonic progress, not erasure). The CLI prints what was deleted; nothing is silent.

**OQ-10 — The current `Cmd::Sync` accepts `--cache <path>` to override the cache location for testing. Do the new `update`/`list`/`show`/`verify`/`gc` commands inherit this, or is the cache path fixed to `~/.terrashift/schemas/`?**

**Default if no reply:** Add `--cache <path>` to all five. Useful for: (a) integration tests using `tempfile`, (b) operators with multiple cache pins per workspace, (c) the eval framework's `.schema-cache/` (which is the same shape but a different root). Default falls through to `~/.terrashift/schemas/`.

---

## 6. Implementation plan reference (NOT a deliverable; for founder context)

Phase 4 (post-confirmation) executes prompt sections 4 and 5 in this order:

1. Land `libs/audit` `SchemaCapture` variant (smallest, isolated). Verify with new chain test.
2. Land `libs/knowledge` `manifest.rs`, `cache.rs`, `list_providers()`. Verify with new schema_cache tests.
3. Land `libs/ai` `SchemasConfig` extension. Verify by parsing the example profile.
4. Land `cli/src/commands/schema/` directory with five subcommands. Verify with new smoke tests.
5. Land `tui/src/commands/schemas.rs` shim + status footer. Verify with new render test.
6. Land `xtask/` crate with `capture-schemas` and `capture-eval-schemas`. Run locally; commit `seed/{provider}/{version}/schema.json` outputs.
7. Land `libs/eval` `required_schemas` extension + new `MissingSchema` error. Run `cargo nextest run -p terrashift-eval` with all 13 fixtures.
8. Delete categorised seed; rewrite the 6 affected tests; update CloudForge docs.
9. Update CI workflows. Run a full CI dry run on a feature branch.
10. UX-pass per prompt section 8.
11. PR open per prompt section 9.

---

## 7. Tested invariants this RFC must preserve

These are existing test-encoded invariants the contractor must not regress during Phase 4:

- `libs/audit/tests/audit_chain_test.rs::tampering_with_signature_is_detected_separately` — the SchemaCapture variant must round-trip through hash chain + signature verification with the same independence (signature tamper ≠ content tamper).
- `libs/audit/tests/audit_chain_test.rs::raw_aws_key_in_payload_panics` — the new SchemaCapture scrub arm must panic on AWS access keys appearing in any of its String fields. The 64-char hex exemption only fires for the `sha256` field; other fields stay strict.
- `libs/knowledge/tests/schema_cache_test.rs::pinned_version_is_immutable` — the runtime cache's `manifest.json` must preserve `captured_at` on re-cache (Article VI). New code paths inherit this discipline.
- `libs/eval/src/runner.rs::run` strict byte equality (per `scorer.rs:127-133`) — new `required_schemas` check fires *before* Generator runs, so any schema-presence failure does not pollute the byte-equality assertion.
- `libs/engine/tests/validator_test.rs::*` — none of these change. Validator's `fetch_schema` path is untouched; only the source of the schemas (cache vs. ad-hoc) changes.
- Article XIII rule 2 (non-monotonic cache behaviour): the manifest is append-only within a `(provider, version)`; `gc` removes entire `(provider, version)` pairs but never modifies existing entries.
- Article XIII rule 5 (redaction boundary): every SchemaCapture audit emission goes through `scrub_or_panic` at `store.rs:152`, same path as every other variant. No bypass.
- Article XIII rule 10 (cache lives at `~/.terrashift/`): the runtime cache is at `~/.terrashift/schemas/`; eval cache is at `terrashift-evals/.schema-cache/` (a fixture, not user data — explicitly different).

---

**End of RFC. Awaiting founder confirmation before proceeding to Phase 4.**

Per prompt section 10: if no reply within 24 hours of this RFC's commit, the OQ-1 through OQ-10 defaults activate and Phase 4 proceeds. Defaults will be cited in the eventual PR description.
