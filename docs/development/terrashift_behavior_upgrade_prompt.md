# Terrashift — Schema Source Migration

**Paste this entire document into Claude Code.** Do not summarise it before pasting. The discipline order — study, design, implement, test, then UX-validate — is the work, not preamble to the work.

---

## 0. The role you are taking

You are a **principal infrastructure engineer**, brought in as a contractor on Terrashift. You have shipped production-grade Terraform tooling. You know `terraform-provider-aws`, `azurerm`, and `google` from the inside. You have debugged provider-schema drift in CI pipelines that ran for years. You have strong opinions about reproducibility, build-time vs. runtime, and the difference between caching and embedding.

You are temperamentally cautious in three specific ways:

1. **You read before you write.** You never modify code you do not fully understand. You walk every file you will touch end-to-end before touching any of them. You take notes.
2. **You do not invent.** You match the existing project's idioms — its module shape, its error type, its trait conventions, its naming. If those conventions are weak or inconsistent, you flag them; you don't quietly impose your own taste.
3. **You delete what you replace.** When you remove a code path, you remove every caller, every config field, every doc reference, every test, every prompt mention of it. The repo never carries a dead branch labeled "old way" alongside a new one.

Treat me (the founder) as a technical peer. Push back when I'm wrong. Ask when something is genuinely ambiguous. Do not ask me to make decisions a quick code read would settle.

This is one continuous engagement. Do not stop after the design phase and wait silently — proceed through the whole sequence below unless you hit something that genuinely requires my input. When you do hit such a thing, stop and ask one specific question, not a bundle.

---

## 1. The change in one sentence

Establish **`terraform providers schema -json` output** (via the already-implemented `TerraformCliSchemaFetcher` in `libs/knowledge/src/schema_fetcher_cli.rs`) as Terrashift's authoritative provider-schema source — captured ahead of time, pinned by version, cached on disk, never executed during a migration run, and refreshable on a cadence the user controls. The HTTP Registry API client (`TerraformRegistryClient`) is **retained for version-metadata listing only** (so users can answer "what versions can I pin to?"); it is not — and never was — on any schema-fetch path.

That is the entire scope. Do not expand it.

---

## 2. Why this change is correct (so you can defend it in code review)

The Registry API publishes **metadata** (provider name, releases, doc URLs). It deliberately does not publish the **schema**. Schema fields — `required`, `optional`, `computed`, `sensitive`, `deprecated`, nested block structure — are produced by the provider binary itself at runtime. Hashicorp's own `terraform validate`, the language server, and every IDE plugin all use `terraform providers schema -json`. It is the only authoritative source.

Concretely for us:
- **Authoritative.** Validator checks exactly what `terraform validate` would check.
- **Reproducible.** Pin a version, get byte-stable JSON.
- **Auditable.** The schema dump is a hashable artifact that lives in the audit log alongside `provider`, `model_id`, and `provider_endpoint`.
- **Article VI compliant.** Article VI bans "latest" in production paths. The Registry API forces "latest" semantics; the schema dump gives us pinning by construction.

**State of the MVP today (verify this in your Phase 3.2 walk):**

- `TerraformRegistryClient` calls `registry.terraform.io` for *metadata only* (versions, namespaces). It has never fetched schemas.
- `TerraformCliSchemaFetcher` *is implemented* in `libs/knowledge/src/schema_fetcher_cli.rs` and runs the exact `tempdir → versions.tf → terraform init → terraform providers schema -json → ProviderSchema` flow this prompt mandates. Its docstring at `registry_client.rs:23-30` claims it "arrives in S4" — that comment is stale; the file exists.
- The seed bundle at `libs/knowledge/seed/` ships ~170 hand-curated `ResourceSchema` JSON files, organised into category subdirectories (`networking/`, `compute/`, `database/`, …) by `libs/knowledge/seed/scripts/import-resource-catalog.mjs` from a CloudForge TypeScript catalog.
- *Nothing* surfaces schema operations to the user via the CLI today. `terrashift schema *` does not exist as a subcommand. The schema cache lives only in tests' `tempdir`s.

This work therefore: **wires the existing fetcher into a production CLI surface; adds a versioned on-disk cache + manifest + audit variant; introduces a build-time bundled-schema xtask; replaces the categorised seed with the flat-per-version layout from section 4.4; deletes only the latest-semantics paths in the Registry client and the categorised seed subdirectories — not the registry HTTP client itself.**

---

## 3. Mandatory study phase — do this BEFORE any code changes

### 3.1. Read the canon

Open and read end-to-end, in this order, taking notes as you go:

1. `docs/reference/stakpak_arch.md` — the upstream architecture reference (per `CLAUDE.md`, this is the canonical in-repo copy; the live Stakpak source under `refs/stakpak/` is for verification only). Pay particular attention to **section 9** (api crate's `SessionStorage` trait — our `SchemaStore` mirrors its shape), **section 22** (checkpoint and resume — same lifecycle hook discipline), **section 32** (CI matrix — where the eval suite plugs in), **section 41 Phase 3** (mirror playbook for swapping a backend), and **section 42** (the ten anti-patterns codified as Article XIII).
2. `docs/architecture/terrashift_plan.md` — sections **3.4** (the eleven seams), **6.X** (model resolution and the policy/preference/locked split — the same convention applies to schema config), **7** (knowledge layer), **24** (constitution).
3. `docs/governance/CONSTITUTION.md` — all thirteen articles. Articles **III, IV, V, VI, VIII, IX, X, XII** and rules **2, 5, 10** of Article XIII govern this change.
4. `docs/development/terrashift_prompts.md` — every prompt P-00 through P-22. The most relevant are **P-06** (Validator), **P-07** (Knowledge service — explicitly references `registry.terraform.io` today and must be rewritten), **P-11** (audit log — needs a new `AuditPayload` variant for schema operations), **P-12** (eval framework), **P-15** (eval suite expansion).

### 3.2. Walk the existing code line by line

For each crate below, open every file and read it. Do not `grep | head`. Take notes. The goal is that by the end of this phase you can describe the project in your own words, in detail, without looking.

1. `libs/knowledge/src/**` — every file. Document the current `SchemaStore` trait, the `LocalSchemaStore` impl, the cache table schema, and every place that talks to `registry.terraform.io`.
2. `libs/engine/src/validator/**` — every file. Document how the Validator consumes the schema today, the `ProviderSchema` Rust type, where validation errors are emitted, and which constitution article each error path implements.
3. `libs/audit/src/**` — every file. Document the current `AuditEntry` and `AuditPayload` enum. You will be adding a `SchemaCapture` variant.
4. `libs/creds/src/**` — every file. The schema-capture path may need to invoke `terraform`, which means resolving `PATH`. Confirm there's no surprise around that.
5. `libs/shared/src/config.rs` — document the `Profile` struct and how providers are configured per profile today.
6. `cli/src/**` — every file. Document where `terrashift init`, `terrashift migrate`, and any existing `terrashift schema` subcommand live, plus the TUI command-routing layer.
7. `tui/src/**` — every file. The TUI is a top-level workspace crate (`tui/`), NOT a subdirectory of `cli/`. Document the slash-command system at `tui/src/commands/mod.rs` and how new commands plug in (existing examples: `audit.rs`, `checkpoint.rs`, `migrate.rs`, `scan.rs`).
8. `terrashift-evals/**` — every golden migration. Document how schemas are referenced in eval fixtures today.
9. `Cargo.toml` (workspace + every crate manifest) — document current dependencies. We may add `which`, possibly `memmap2`, possibly `humansize`. No others without justification.

### 3.3. Write the RFC, then stop

Produce `docs/RFC-schema-source-migration.md` with the following sections, then **stop and wait for me to read it.** Do not begin Phase 4 until I confirm.

```markdown
# RFC — Schema source migration

## Findings (one paragraph per crate touched)
…

## Where the Registry API path lives today
File + line ranges of every reference.

## Concrete change list (every file that will be created, modified, or deleted)
Group by crate. Mark each entry [NEW], [MODIFY], or [DELETE].

## Surprises and concerns
Anything that smells wrong in the existing code. Be honest.

## Open questions for the founder
Specific ambiguities only the human can resolve. Numbered. Each with your recommended default if I don't reply within 24h.
```

If I do not reply within 24 hours, proceed using your stated defaults and note them in the PR description.

---

## 4. The contract you are building

### 4.1. Schema acquisition — three modes, never on the migrate hot path

Schema is captured by running the user's local `terraform` binary against a pinned `providers.tf`. There are three legitimate moments:

| Mode | Triggered by | When it runs | Latency budget |
|---|---|---|---|
| **A. Bundled** | `cargo build --release` in CI | Once per Terrashift release | N/A — build time |
| **B. User-initiated** | `terrashift schema update` | When the user explicitly asks | Up to 60 s per provider, with progress |
| **C. Auto-update** | Background check on `terrashift migrate` startup | Per profile cadence (off / weekly / monthly) | Non-blocking — migrate proceeds with current cache; new schemas land for the *next* run |

**Schema capture NEVER runs during `terrashift migrate`.** That path is offline-capable and fast. If a migrate command requests a schema version that is not cached, it FAILS LOUD with an actionable message. Article IV: failures are loud.

```
error: schema for aws ~> 5.30 not cached in this profile.
       run: terrashift schema update --provider aws --version "~> 5.30"
       or:  terrashift schema update --all
```

### 4.2. The CLI surface

Add to `cli/src/commands/`:

```
terrashift schema list
  Show cached schemas: provider · version · captured · size · sha256 prefix.
  Tabular, terse, monospace-aligned.

terrashift schema update [--provider <name>] [--version <constraint>] [--all] [--non-interactive]
  Capture or refresh schemas.
  No flags + interactive: prompt user with current cache state and let them
    select providers/versions to update via the TUI.
  --all: refresh every cached (provider, version) pair.
  --non-interactive: take all defaults, no prompts; for CI use.
  Requires `terraform` on PATH. If missing, print a platform-aware install
    hint (brew on macOS, apt on Debian/Ubuntu, choco on Windows, plus the
    terraform.io download link) and exit non-zero.
  Per-provider 60s timeout. Stream progress.

terrashift schema show <provider>@<version> [--filter <prefix>]
  Print a human-readable summary: resource count, data-source count,
  top-level resource list (alphabetical, paginated), deprecated attribute count.
  --filter <prefix> narrows the resource list to types starting with the prefix
    (e.g. --filter aws_vpc). This is how users find specific services without
    service-category grouping.
  When stdout is not a TTY, output is plain text (no ANSI, no pagination)
    so the command is pipe-friendly: `terrashift schema show aws@5.30.0 | grep iam`.

terrashift schema verify
  SHA-256 every cached schema, compare against the manifest.
  Report drift. Exit 1 if any schema is corrupt or missing. Used by CI.

terrashift schema gc [--keep-versions <N>]
  Delete schemas not referenced by any profile. --keep-versions defaults to 2
  (current + previous per provider).
```

In the TUI, register the slash command **`/schemas`** that opens an inline picker showing the same data as `terrashift schema list`, with keyboard navigation to trigger an update on the highlighted row.

### 4.3. Configuration — policy/preference/locked split

Mirror the same convention used for model resolution (section 6.X of `terrashift_plan.md`). Add to `libs/shared/src/config.rs`:

```toml
[profiles.default.schemas]
  # PREFERENCE — user overridable
  pinned = [
    { provider = "aws",     version = "~> 5.30" },
    { provider = "google",  version = "~> 5.40" },
    { provider = "azurerm", version = "~> 3.110" },
  ]

  # POLICY — operator-locked at profile level. Defaults to "off" for the MVP.
  auto_update = "off"   # one of: "off" | "weekly" | "monthly"
  auto_update_window = "off-hours"   # honored only when auto_update != "off"

  # LOCKED — never overridable
  # cache_root, manifest format, signing — not exposed in config at all
```

Auto-update default is **off** for the MVP. Explicit user control wins until we have telemetry showing background updates don't surprise people. Document this default in the RFC and call it out specifically in the PR.

### 4.4. The on-disk cache

Cache root: `~/.terrashift/schemas/`

**Layout decision: flat per-provider, no service categories.** The existing seed data is organised into `networking/`, `compute/`, `iam/`, etc. subdirectories — that organisation is going away. `terraform providers schema -json` produces one flat list of resource types per provider; service categories are an editorial property of how a human would like to browse the data, not a property of the data. Mixing those concerns at the storage layer is exactly the kind of build-time-vs-editorial coupling Article XIII catches. Resources are identified by `(provider, resource_type)` everywhere in the codebase already; categories are not on any code path. If, in 6 months, a UX moment genuinely needs categorisation that alphabetical-with-search can't serve, it gets added then with a sidecar — not pre-built speculatively now.

Layout:
```
~/.terrashift/schemas/
  manifest.json
  aws/
    5.30.0/
      schema.json          # one flat extracted schema, all resource types
      capture.log          # terraform init/providers schema stdout+stderr
  google/
    5.40.2/
      schema.json
      capture.log
  azurerm/
    3.110.0/
      schema.json
      capture.log
```

`schema show <provider>@<version>` sorts alphabetically and lets the user pipe to `grep`. No category grouping, no `_uncategorized` bucket, no sidecar files.

`manifest.json` schema (use `serde_json` + a typed Rust struct; do NOT free-form):

```json
{
  "schema_version": 1,
  "schemas": [
    {
      "provider": "aws",
      "source": "hashicorp/aws",
      "version": "5.30.0",
      "version_constraint": "~> 5.30",
      "captured_at": "2026-05-07T11:00:00Z",
      "captured_by": "schema_update_command",
      "terraform_version": "1.7.5",
      "sha256": "a4f7e21d8c…",
      "size_bytes": 41258032,
      "schema_path": "aws/5.30.0/schema.json"
    }
  ]
}
```

The `schema_version` field lets us evolve the manifest format in a backward-compatible way later. Bump it when the JSON shape changes; ship a one-shot migration in `libs/knowledge/src/manifest/migrations.rs`.

### 4.5. Hot-path access pattern

The Validator and Mapper read **only the manifest** at startup. The actual `schema.json` files are loaded **lazily on first reference** for a given `(provider, version)` pair, then held in an `Arc<ProviderSchema>` for the life of the migration. The pattern is `let schema: Arc<ProviderSchema> = Arc::new(serde_json::from_slice(&fs::read(path)?)?);` followed by `Arc::clone(&schema)` to share across worker threads at zero copy cost. **Do not use `memmap2`** — it does not help when the consumer is a typed `serde_json` deserialise (the parser walks every byte once anyway, and the resulting tree lives on the heap regardless of how the input bytes were obtained).

A 40 MB JSON parse is a hard ceiling — under no circumstances should the migrate hot path do this on every invocation. If your benchmark shows it does, the design is wrong. Cache the parsed `Arc<ProviderSchema>` per `(provider, version)` in a `dashmap` or `tokio::sync::OnceCell` keyed in `KnowledgeService`.

### 4.6. Bundled schemas

`cargo build --release` runs an `xtask` (or `build.rs` — see Phase 4 decision below) that:

1. Reads `libs/knowledge/seed/manifest.toml` for the version pins (build-time pins file; distinct from — and **not** to be confused with — the runtime cache manifest at `~/.terrashift/schemas/manifest.json` from section 4.4).
2. In `target/build-schemas/`, writes a `providers.tf` matching those pins.
3. Runs `terraform init && terraform providers schema -json`.
4. Splits the result by provider using `serde_json` (not shelling out to `jq` — we cannot rely on `jq` being on a developer's machine).
5. Writes outputs to `libs/knowledge/seed/{provider}/{version}/schema.json`. This **replaces** the existing `libs/knowledge/seed/{provider}/{category}/{resource_type}.json` layout from CloudForge — the per-category, per-resource files are deleted as part of this migration (see section 4.9). One `schema.json` per `(provider, version)` is the new shape.
6. The release binary embeds these via `include_bytes!`. On first run, Terrashift extracts them into `~/.terrashift/schemas/` if and only if those files don't already exist there.

This makes the binary self-contained for the GCP→AWS demo path. New users get a working cache with zero `terraform` invocation. Power users override with `terrashift schema update`.

**Phase 4 decision.** `xtask` vs `build.rs`: prefer `xtask`. `build.rs` would force every developer building Terrashift from source to have `terraform` installed. `xtask` makes schema capture an explicit step (`cargo xtask capture-schemas`) that CI runs but day-to-day developers don't need. Document this in the RFC and propose `xtask` unless you find a reason against it during the study phase.

### 4.7. Audit log

Add a new variant to `AuditPayload` in `libs/audit/src/entry.rs`:

```rust
SchemaCapture {
    provider: String,            // "aws"
    source: String,              // "hashicorp/aws"
    version_constraint: String,  // "~> 5.30"
    resolved_version: String,    // "5.30.0"
    terraform_version: String,   // "1.7.5"
    captured_via: CaptureVia,    // Bundled | UserCommand | AutoUpdate
    sha256: String,
    duration_ms: u32,
}
```

Emit one entry per provider per capture invocation. Cite Article V in the PR description — every schema in the system is now traceable to who captured it, when, with which Terraform version.

### 4.8. Auto-update behaviour

When `auto_update != "off"` and `terrashift migrate` starts:

1. **Non-blocking.** Spawn a tokio task to check for newer versions matching the user's constraints. The migration proceeds immediately with the current cache.
2. **Cadence-aware.** Last-attempt timestamp lives in the manifest as `last_auto_check`. Skip if within the cadence window.
3. **Confirms before applying.** If a new version is found, the next time the user runs an interactive command (not a script in `--non-interactive` mode), Terrashift prompts: `aws 5.30.0 → 5.30.4 available. Capture now? [Y/n]`. Never silently swap a pinned schema — Article VI demands version-pinned migrations are reproducible, which means the user authorizes every schema change.
4. **Background failures are non-fatal but logged.** A failed auto-update writes an `AuditPayload::SchemaCapture` entry with `outcome: Err`, plus a `tracing::warn` line. It does not abort the migration.

### 4.9. What gets DELETED

This is the part where you must be ruthless. After the new path is wired and tested, search the repo for and remove every trace of the old approach. At minimum:

- The **schema-fetch paths** in `libs/knowledge/src/registry_client.rs` (this is a **single file**, not a directory — verify against your Phase 3.2 walk). Specifically: any code that resolves a *latest* schema, any path that returns a `ProviderSchema` from the HTTP client, and the stale "`TerraformCliSchemaFetcher` arrives in S4" docstring at line 23-30 (the impl already exists). **Retain** the metadata-listing methods (`ProviderMetadata`, `ProviderVersions`, `ProviderVersionEntry` and their fetch functions) — `terrashift schema update` needs them to answer "what versions can I pin to?" before the user picks a constraint.
- Any `reqwest` calls to `registry.terraform.io/v1/providers/{ns}/{name}/{version}/download-url` (or any endpoint claiming to return schemas — there isn't one, but defensive grep regardless). Calls to `/versions` and metadata endpoints stay.
- Any test fixtures that mock the Registry API at the schema layer (the metadata-mocking fixtures stay).
- Config fields like `registry_endpoint`, `registry_timeout`, `registry_retry_*` may be retained if they govern the metadata HTTP client; remove only if scoped to the schema path.
- The existing **categorised seed subdirectories**: `libs/knowledge/seed/aws/networking/`, `libs/knowledge/seed/aws/compute/`, `libs/knowledge/seed/aws/database/`, and the equivalents under `azurerm/` and `google/` (the directory tree contains `analytics/`, `compute/`, `containers/`, `database/`, `developer-tools/`, `machine-learning/`, `management/`, `messaging/`, `networking/`, and more — walk the tree in your Phase 3.2 study and list every category subdirectory you find). The CloudForge-derived per-category, per-resource layout is editorial overlay, not data shape, and goes away entirely with this migration. The new flat-per-(provider, version) layout from section 4.4 replaces it.
- The **CloudForge importer** at `libs/knowledge/seed/scripts/import-resource-catalog.mjs` and any documentation that points contributors at it. The seed is now produced by the `cargo xtask capture-schemas` path, not by a Node.js script over an external TypeScript catalog.
- Any code that walks the categorised seed structure (look for `read_dir` / `WalkDir` calls in `libs/knowledge/src/`, `libs/knowledge/seed/`, and `cli/src/commands/`). The new code reads exactly one file per `(provider, version)` — no directory walking on the schema path.
- Any sidecar mapping files like `categories.toml`, `service_index.toml`, `category_map.json` if they exist in `libs/knowledge/seed/`. They're editorial artifacts of the old approach.
- Documentation references in `README.md`, `docs/**`, and inline `///` comments — including any guidance that tells contributors to "add new resources under the right service category."
- Mention of the Registry API path in `terrashift_prompts.md` P-07 (rewrite that prompt — see Phase 8 below)

Do not leave a `// TODO: remove old registry path` comment. Do not leave a feature flag that gates one against the other. Do not keep the old code "for reference." Do not leave one stub category subdirectory "in case we need it later." Git remembers; the working tree should not.

If you find a reference and aren't sure whether removing it is safe, list it in the RFC under "Surprises and concerns" — don't quietly leave it.

---

## 5. terrashift-evals — what to change there

The eval suite already exists. It needs three updates:

### 5.1. Schema fixtures

**State today (verify in Phase 3.2):** the eval framework at `libs/eval/src/runner.rs` is purely byte-stable codegen — it loads `MappingPlan` JSON, runs the Generator into a tempdir, and compares against `expected/`. The runner's docstring even admits *"Token cost = 0 placeholder; real Mapper integration arrives in S4"*. Schemas are **not currently fetched in evals**; the Validator-with-real-schema path is forward-looking. This section therefore **introduces** schema participation in evals as part of S4 prep — it isn't migrating a pre-existing schema fetch in evals; it's wiring one.

Every golden migration declares its required schemas in its `manifest.toml`. **Extend the existing `GoldenManifest` struct at `libs/eval/src/golden.rs:25-40` with a `required_schemas: Vec<RequiredSchema>` field. Do not introduce a new file format or a parallel reader.** Define `RequiredSchema` next to it as a `#[derive(Deserialize)]` with `provider: String` and `version: String` fields. Add the field with `#[serde(default)]` so existing fixtures continue to load:

```toml
required_schemas = [
  { provider = "aws",    version = "~> 5.30" },
  { provider = "google", version = "~> 5.40" },
]
```

The eval harness in `libs/eval/src/runner.rs`:
1. Reads `required_schemas` for the migration.
2. Confirms each is present in the test fixture's schema cache (a separate dir from the user's `~/.terrashift/schemas/`, to keep CI hermetic).
3. If anything is missing, the eval fails fast with `error: golden migration foo requires aws ~> 5.30 — run: cargo xtask capture-eval-schemas`.

### 5.2. Eval-fixture schema capture

Add `cargo xtask capture-eval-schemas`. This is a separate xtask from the bundled-schemas one. It captures the union of all `required_schemas` declared by every golden migration in `terrashift-evals/` into `terrashift-evals/.schema-cache/`. The directory is checked into git **as compressed JSON** (`zstd -19`, ~10× compression for typed JSON) so CI doesn't run `terraform` and doesn't depend on the network.

### 5.3. Updated assertions

For each golden migration, add a new assertion: `audit_must_contain_schema_capture` — checks that the audit log records which schema versions were used. This proves the new schema path is on the audit boundary (Article V). Add this assertion to all existing migrations during the migration; if any pre-existing migration already validates schema-related fields, reconcile both into one assertion block.

### 5.4. CI matrix

Update `.github/workflows/ci.yml` (or whatever lives there per your study notes):
- Job A: `cargo xtask verify-eval-schemas` — runs `terrashift schema verify` against the eval fixture cache. Fails the build if any cached schema is corrupt or out of date with the manifest.
- Job B: `cargo nextest run -p terrashift-eval` — runs the full golden migration suite (the package is `terrashift-eval`, singular — `libs/eval/Cargo.toml`; the `terrashift-evals/` directory is the fixture corpus, not a package).
- Job C (new): `cargo xtask refresh-eval-schemas --check` — runs nightly. If a newer patch version of any pinned provider is published, opens a PR that updates the cache. The PR triggers the eval suite; if anything breaks, the PR is blocked. This is Article VI enforcement automated.

---

## 6. Documents to update

After the code is in and green, update:

1. `terrashift_plan.md` section 7 — add subsection 7.X "Schema acquisition pipeline" describing the three modes, the manifest format, the cache layout. Cite the relevant constitution articles.
2. `terrashift_prompts.md` P-07 — rewrite. The current text says "calls registry.terraform.io and populates cache." That sentence does not exist anymore. The new P-07 references the schema-capture xtask and the bundled-schemas extract path.
3. `CONSTITUTION.md` — no article changes, but if you find an interaction with Article XIII you didn't expect, propose a new rule entry under Article XIII and flag it for me.
4. `README.md` — single section explaining the schema model to a new user: how it works, when it captures, what `terrashift schema update` does.
5. `docs/RFC-schema-source-migration.md` — already exists from Phase 3. Mark "Status: Implemented" with the merge commit SHA.

---

## 7. Test the MVP from every direction

Once code, evals, and docs are in, **before opening the PR**, run these tests in order. Keep going until each one passes. If one fails, fix it; do not skip to the next.

### 7.1. Unit tests

- `cargo nextest run --workspace` — every crate.
- `cargo nextest run -p terrashift-knowledge -- --include-ignored` — covers the slow tests that actually invoke `terraform`. (Package name is `terrashift-knowledge` per `libs/knowledge/Cargo.toml:2`; the workspace convention is `terrashift-<crate>`, never the path.)

### 7.2. Integration tests

- Spin up a fresh `~/.terrashift/` (use `tempfile`). Run `terrashift init`, `terrashift schema list` (should show bundled schemas), `terrashift schema update --provider aws --version "~> 5.30"`, `terrashift schema show aws@5.30.0`, `terrashift schema verify`, `terrashift schema gc`. Each should produce sensible output and exit 0.
- Force-corrupt one of the cached `schema.json` files (truncate it). Run `terrashift schema verify`. Should exit 1 with a clear message naming the corrupt file.
- Delete the manifest. Run any migrate command. Should exit with a clear "no schemas cached, run `terrashift schema update`" error.
- With no `terraform` on PATH, run `terrashift schema update`. Should print the platform-specific install hint and exit non-zero — not panic.
- With network disconnected, run a migrate with cached schemas. Should succeed — schemas are local.
- With `auto_update = "weekly"`, run a migrate twice in a row. The second run should not re-check (cadence skip). Run `touch -d "8 days ago" manifest.json` and re-run; should re-check.

### 7.3. End-to-end golden migration

- Run the GCP→AWS demo migration end-to-end. Confirm the audit log contains a `SchemaCapture` entry for each provider used. Confirm the Validator catches a hand-introduced hallucination (modify the Mapper output to insert an attribute that doesn't exist in the schema; Validator should fail loud per Article III).

### 7.4. Eval suite

- `cargo xtask verify-eval-schemas` — green.
- `cargo nextest run -p terrashift-eval` — every golden migration green (singular — see note on line ~341).
- Token cost regression check — current run vs baseline must be within ±30% (Article XII rule 4).

### 7.5. Constitution sanity check

Walk through every article and convince yourself the new path satisfies it. Write a one-line note per article in the PR description. Be specific:
- III: the Validator still uses the schema as truth.
- IV: every error path is loud and actionable.
- V: schema capture writes to the audit log; the actor and Terraform version are recorded.
- VI: schemas are pinned, never "latest" in production.
- VIII: this change does not move us between stages.
- IX: cached schemas are user data — `terrashift schema gc` defaults to keeping at least N=2 versions per provider.
- X: every capture, every read, every cache miss has a `tracing` span with structured fields.
- XII: capture is a build-time artifact, not a runtime LLM call. Token economy unaffected.
- XIII rule 2: no non-monotonic cache behaviour — the manifest is append-only within a version.
- XIII rule 5: if any path of the schema capture passes through the LLM (it shouldn't), the redaction layer must still be on it.
- XIII rule 10: the schema cache lives at `~/.terrashift/`, not anywhere else.

---

## 8. End-user UX testing — be the user

Every test in section 7 was an engineer's test. Now switch hats. You are now a Terrashift user who has just downloaded the binary. You have not read the docs. Your job is to make a GCP→AWS migration work. Walk through the experience and react to it as a person, not a function.

For each touchpoint below, ask three questions:
- **Did I understand what just happened?**
- **Did I know what to do next?**
- **Was anything more painful than it had to be?**

Touchpoints to walk:
1. Fresh install. Run `terrashift` with no arguments. What do I see? Does it tell me how to begin?
2. `terrashift init`. Does it explain what just happened and what's now on my disk? Does it surface the schema cache existence?
3. `terrashift schema list` on a brand-new install. The bundled schemas — are they obvious? Is the formatting readable in a 80-char terminal?
4. `terrashift schema update` interactive. Does the prompt make sense without docs? When `terraform` is missing, is the install hint genuinely helpful or boilerplate?
5. The 60-second progress UI during capture. Does it tell me what's happening? Does it estimate time-to-finish? Can I tell when it's stuck vs. working?
6. A migrate that fails because a schema is missing. Read the error message out loud. Does it tell me exactly what to type next?
7. A migrate where auto-update finds a new version. Is the prompt clear? Do I understand what changing the schema version implies?
8. `terrashift schema show aws@5.30.0`. AWS at 5.30 is ~1,400 resource types. Is the output something a real engineer would paste into a Slack thread, or is it a wall of text? Default behaviour is alphabetical with pagination; confirm `--filter <prefix>` (e.g. `--filter vpc`) works for the common "I'm looking for a specific service" case. Confirm the output is grep-friendly when piped (no ANSI when stdout is not a TTY). Without service-category grouping, search has to be excellent — make sure it is.
9. `terrashift schema verify` finds drift. Is the message blameful or helpful?

For every painful moment, fix it. Update the error string, the help text, the progress phrasing, the confirmation prompt, the column widths. Then walk it again. Continue until each touchpoint reads cleanly to you as a user.

The standard is not "it works." The standard is **comfortable**. A senior engineer should never feel patronised, and a junior engineer should never feel lost.

When you've made a UX pass, write up a short `docs/UX-pass-2026-05.md` listing every change you made during this phase. That document is a deliverable.

---

## 9. Output

Open one PR. Title: `feat(knowledge): replace Registry API with terraform providers schema -json`.

PR description must contain:

1. One-paragraph summary of the change.
2. The constitution checklist from section 7.5.
3. List of every file created, modified, or deleted, grouped by crate.
4. The output of the section 7.2 integration test sequence (paste the terminal session — not screenshots).
5. The list of UX changes from section 8.
6. The list of follow-up work that is genuinely out of scope, with my requested decision on each.

The PR is rebased on `main` and squash-mergeable. CI is green. Article VII (PRs ≤ 500 lines preferred) — if this exceeds 500 lines, that's expected for a migration like this; explicitly note the line count and which sub-modules are responsible for the bulk of it.

---

## 10. Behaviour expectations

- **Stop at section 3.3.** Wait for me to read the RFC before implementing. I'll either reply or — if I'm offline — your stated defaults apply after 24 hours.
- **Do not stop again until section 9.** The middle is one continuous engagement.
- **Ask one question, not five.** When you do need clarification, scope it tightly.
- **No suggestions. Just answers.** If I tell you "this is wrong," fix it. Don't list five options for how to fix it.
- **Cite the constitution by article and rule number** in every commit message that touches a constitutional concern. PR reviewers should be able to scan commits and see the rules being respected.
- **Delete what you replace.** Section 4.9 is not optional.

Begin with section 3. Read first.