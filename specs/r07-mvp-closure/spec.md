# Feature Specification: MVP Closure — Mapper prompt context-injection + default-output convention

**Feature Branch**: `feat/r07-mvp-closure`
**Created**: 2026-05-08
**Status**: Draft
**Input**: User description: "i want the mvp to be able to do a full migration e2e so if i have aws tf the mvp should generate a new folder with the name of the target cloud"

---

## Background

After the Stage 1 binary shipped (v0.1.0) and the schema-source-migration PR (#6) landed, an end-user e2e test on a 5-resource AWS module fixture surfaced that **0 of 5 resources actually emit to disk** during `terrashift migrate aws → azurerm`. Three classes of failure (documented in `docs/UX-pass-2026-05.md` follow-up):

1. **Mapper omits required attributes.** 3 of 5 resources had correct target type but failed Validator on missing `name` attribute (Azure networking resources require `name`).
2. **Mapper picks unsupported target types.** `aws_instance` → `azurerm_virtual_machine` (deprecated; template registry only supports `azurerm_linux_virtual_machine`).
3. **Mapper accepts empty `target_type`.** Silent corruption: empty entry in proposed mappings, then `template miss for ''` two layers downstream.

Per Article I, Recovery agent (Stage 2 / S10) is the constitutional answer to retry-on-failure. This spec is the **Stage-1 bridge** that closes most of the gap without crossing the no-agents line: tighten the Mapper prompt + add Mapper-side validation. Plus the orthogonal default-output UX fix.

## stakpak_arch.md References

- §10 (LLM SDK / structured output discipline)
- §27 (secret detection / redaction — adjacent invariant; no leakage in new prompt context)

## Constitution Articles in Scope

- **Article I** — Mapper stays "LLM-with-structured-output" (single call, no loop). NOT introducing an agent.
- **Article III** — Validator gate unchanged; this spec reduces upstream noise hitting it.
- **Article IV** — empty-`target_type` becomes a loud Mapper-side error rather than a downstream Generator confusion.
- **Article XII rule 2** — prompt change invalidates the Mapper cache by design (the SYSTEM_PROMPT version string at `prompt.rs:31` is the cache key half).
- **Article XIII rule 1** — `ContextReducer` still on the prompt path (compile-enforced today; preserved).

## User Scenarios & Testing

### User Story 1 — Mapper produces complete attributes (Priority: P1)

A user with AWS Terraform runs `terrashift migrate --from aws --to azurerm` and gets target HCL where every Azure resource has its required attributes set (e.g., `name` on `azurerm_virtual_network`).

**Why this priority**: Without this, ≥60% of mapped resources fail Validator and emit nothing. This is the load-bearing fix that turns a non-functional migrate into a functional one.

**Independent Test**: Run the e2e fixture at `fixtures/e2e-aws-to-azure/` against the rebuilt binary. Today: 3 of 5 resources fail with "missing required attribute 'name'". Target: 0 of 5 fail this way.

**Acceptance Scenarios**:

1. **Given** an AWS `aws_vpc` resource, **When** Mapper runs with the new prompt, **Then** the produced `MappedResource` has `attributes["name"]` set (string or reference value).
2. **Given** an AWS `aws_security_group` resource, **When** Mapper runs, **Then** the produced `azurerm_network_security_group` has both `name` AND `resource_group_name` set.
3. **Given** the schema cache has an entry for `azurerm_virtual_network` with `required: ["name", "resource_group_name", "address_space", "location"]`, **When** the Mapper prompt is built, **Then** the prompt includes those four attribute names in a "MUST set" block.

### User Story 2 — Mapper picks supported target types only (Priority: P1)

A user gets target HCL where every emitted resource is one the Generator's template registry can serialize (no `template miss for target_type 'X'` errors).

**Why this priority**: Without this, even a perfect attribute set is wasted if `target_type` doesn't have a template. The LLM defaults to deprecated names (e.g. `azurerm_virtual_machine`) without explicit guidance.

**Independent Test**: Same fixture. Today: 1 of 5 resources hits "template miss for target_type 'azurerm_virtual_machine'". Target: 0 of 5.

**Acceptance Scenarios**:

1. **Given** the prompt is built for an `aws → azurerm` migration, **When** the LLM is instructed, **Then** the prompt includes a `VALID TARGET TYPES` block listing exactly the keys of `TemplateRegistry.stage1()` filtered to azurerm-prefix.
2. **Given** an `aws_instance`, **When** Mapper runs, **Then** the produced `target_type` is `azurerm_linux_virtual_machine` (in the registry) NOT `azurerm_virtual_machine` (deprecated, not in registry).
3. **Given** the LLM nevertheless returns a `target_type` not in the supported set, **When** Mapper validates, **Then** the resource is rejected with `MapperError::UnsupportedTargetType { source_addr, target_type, suggestion }` containing the closest match from the supported set.

### User Story 3 — Empty target_type is rejected at the Mapper boundary (Priority: P1)

A user never sees a blank-target-type entry in the proposed-mappings table.

**Why this priority**: Today, the LLM occasionally emits `{"target_type": "", "target_addr": ""}` and Mapper accepts it. This is silent corruption — the user sees a blank row in output, then a confusing `template miss for ''` downstream.

**Independent Test**: Synthetic test: feed Mapper a stubbed LLM response with one resource having `target_type: ""`. Today: passes through. Target: rejected with `MapperError::EmptyTargetType`.

**Acceptance Scenarios**:

1. **Given** an LLM response containing a `MappedResource` with `target_type == ""`, **When** Mapper parses, **Then** an `Err(MapperError::EmptyTargetType { source_addr })` is returned (not `Ok(plan)` with the malformed entry).
2. **Given** the same input, **When** the error is displayed to the user, **Then** the message names the source resource that was orphaned.

### User Story 4 — Default output folder follows convention (Priority: P2)

A user runs `terrashift migrate --from aws --to azurerm --source ./aws-app` without `--output` and sees emitted files in `./aws-app-azurerm/` (next to the source), not in a tempdir that disappears.

**Why this priority**: Lower than US1-3 because it's a UX issue not a correctness one. Today, `--output` defaults to a tempdir that gets garbage-collected on exit; the user has to specify `--output ./somewhere` to keep results. Convention-over-configuration: target cloud is the natural default folder name.

**Independent Test**: Run `terrashift migrate --source X --from aws --to azurerm` with no `--output`. Today: tempdir, files lost. Target: `<X>-azurerm/` next to source, files preserved.

**Acceptance Scenarios**:

1. **Given** `--source ./aws-app` and `--to azurerm` and no `--output`, **When** migrate runs successfully, **Then** files are at `./aws-app-azurerm/`.
2. **Given** `./aws-app-azurerm/` already exists with files in it, **When** migrate runs, **Then** existing files are merged-with (current behavior, preserved).
3. **Given** `--output /custom/path` is passed explicitly, **When** migrate runs, **Then** the explicit path wins (no regression).

### Edge Cases

- **Empty schema cache**: Mapper prompt-context injection must gracefully handle the case where `KnowledgeService::fetch_schema(target_provider, "...")` returns `Err(NotFound)` — prompt falls back to the resource-type set without per-attribute requirements (less precise but still functional).
- **Source path is a single file, not a directory**: `--source ./main.tf` — the parent dir's name + `-azurerm` may produce surprising paths. Reject with a clear error or fall back to `terrashift-output-azurerm/`. Decided: reject with `error: --source must be a directory`.
- **Source is the current working directory** (`.`): the default-output computation can't use the parent's name. Decided: fall back to `./<target>-output/` when source is `.` or one level up.

## Requirements

### Functional

- **FR-1**: The Mapper system prompt MUST include a per-target-resource-type block listing required attributes from the cached schema, when the schema cache has an entry for that target type.
- **FR-2**: The Mapper system prompt MUST include a `VALID TARGET TYPES` enumeration constructed from `TemplateRegistry.stage1().keys()` filtered by target-provider prefix.
- **FR-3**: The Mapper MUST reject `MappedResource` entries with `target_type.is_empty()` at parse time, returning `MapperError::EmptyTargetType { source_addr }`.
- **FR-4**: The Mapper MUST reject `MappedResource` entries whose `target_type` is not in the supported set, returning `MapperError::UnsupportedTargetType { source_addr, target_type, suggestion }`. Suggestion is the closest match from the supported set (Levenshtein distance ≤ 3, or `None`).
- **FR-5**: The `terrashift migrate` command, when invoked without `--output`, MUST default to `<source-stem>-<target-provider>/` next to the source. Existing `--output <path>` behavior is preserved.
- **FR-6**: The `SYSTEM_PROMPT` version string MUST bump from `v1` to `v2` per `prompt.rs:30-32` cache-stability discipline. Article XII rule 2.

### Non-Functional

- **NFR-1**: The new prompt must NOT increase token cost by >2× on the e2e fixture (5 resources, ~3KB inventory). Article XII rule 4. Measure via the Mapper's existing `LlmClient::complete` returns the metadata.
- **NFR-2**: Mapper-side validation must run on the parse-then-validate path; no second LLM call.
- **NFR-3**: The default-output behavior must be back-compatible for any caller passing `--output` explicitly.

## Success Criteria

- **SC-1**: On `fixtures/e2e-aws-to-azure/`, `terrashift migrate --from aws --to azurerm` produces ≥4 of 5 resources to disk (current state: 0 of 5).
- **SC-2**: All new Mapper validation errors include the offending `source_addr` in the message — verifiable via grep over `MapperError::Display` impls.
- **SC-3**: `terrashift migrate --source X --from aws --to azurerm` with no `--output` writes files to `X-azurerm/`, verified by an integration test that calls migrate and checks the path.
- **SC-4**: `cargo test --workspace` passes; clippy clean; fmt clean.
- **SC-5**: `cargo audit` clean (no new RUSTSEC advisories from any dep change).

## Article XIII Rules in Play

- **Rule 1** (no bypass of message conversion pipeline) — preserved; `reducer.reduce(messages)` still on the path.
- **Rule 2** (cache stability) — `SYSTEM_PROMPT` version string bump invalidates prior Mapper cache by design.
- **Rule 3** (no unwrap/expect/string_slice in production) — applies; new error variants use `thiserror`, no panics.

## Out of Scope (defer to S10 Recovery agent)

- **Bounded retry on Validator failure**: When Validator reports a missing attribute the Mapper failed to produce, S10's Recovery agent re-prompts with the failure context. NOT in this spec; that's the agent line and Article I requires RFC + stage-gate approval.
- **Constraint-driven LLM output (guided decoding)**: HuggingFace/Together support is uneven; defer until Stage 3 LLM tier router lands provider fallback.
- **Schema-aware Mapper for resources outside the supported template set**: today the supported set is ~17 types. Expanding the set is `terrashift-mappings` repo work (P-18), separate cadence.
