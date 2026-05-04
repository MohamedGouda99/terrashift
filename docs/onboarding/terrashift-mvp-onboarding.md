# Terrashift — MVP Onboarding Guide

**Audience:** an expert ML engineer joining the project at the MVP-ready milestone.
**Goal:** by the time you finish reading this, you should be able to
clone the repo, run `terrashift` against the bundled fixture, identify the
two or three places your ML expertise will move the needle, and know which
files to read before opening your first PR.

**Last updated:** 2026-05-04
**Project status:** Stage 1 + interactive TUI shipped. Stage 2 (Recovery,
Cost Optimizer agents) not yet started.

---

## Table of contents

1. [The 30-second pitch](#1-the-30-second-pitch)
2. [Why this project exists](#2-why-this-project-exists)
3. [The thesis: deterministic-first, LLM where it earns its place](#3-the-thesis-deterministic-first-llm-where-it-earns-its-place)
4. [The constitution at a glance](#4-the-constitution-at-a-glance)
5. [Architecture in one diagram](#5-architecture-in-one-diagram)
6. [Repository layout](#6-repository-layout)
7. [The migration pipeline — component-by-component](#7-the-migration-pipeline--component-by-component)
8. [The knowledge layer (RAG) — your home turf](#8-the-knowledge-layer-rag--your-home-turf)
9. [LLM integration](#9-llm-integration)
10. [The CLI surface](#10-the-cli-surface)
11. [The interactive TUI](#11-the-interactive-tui)
12. [Test landscape & MVP scorecard](#12-test-landscape--mvp-scorecard)
13. [Stage roadmap](#13-stage-roadmap)
14. [Where your ML expertise plugs in](#14-where-your-ml-expertise-plugs-in)
15. [Onboarding checklist (Day 1 → Week 1)](#15-onboarding-checklist-day-1--week-1)
16. [How we work — Spec Kit workflow](#16-how-we-work--spec-kit-workflow)
17. [Reference materials](#17-reference-materials)
18. [Glossary](#18-glossary)

---

## 1. The 30-second pitch

Terrashift is a CLI tool that migrates Terraform-managed infrastructure
between cloud providers — AWS to Azure, Azure to GCP, GCP to AWS, and so
on. It uses LLMs only where they earn their place (cross-cloud equivalence
mapping, recovery from failed migrations, cost analysis), and pure Rust
everywhere else (HCL parsing, schema validation, file emission, sandboxed
`terraform apply`). It ships as a single statically-linked Rust binary.

You type:

```powershell
terrashift migrate --source ./aws-prod --from aws --to azurerm --output ./azure-prod
```

…and you get an Azure-flavoured Terraform tree out, with an audit log of
every LLM call, every cost-impacting decision, and every file written.

Or you type just `terrashift` and you get an interactive ratatui session
like `claude` or `stakpak` — slash commands, scroll-back history, the lot.

---

## 2. Why this project exists

Cross-cloud Terraform migration today costs **$500K–$2M per project**,
takes **6–12 months**, and routinely fails halfway through `terraform
apply` because someone hallucinated a target attribute that doesn't exist
in the destination provider. Existing approaches fall into three buckets:

1. **Manual rewrite by consultants** — slow, expensive, error-prone, and
   the deliverable rots the day someone updates the source estate.
2. **General-purpose LLM agents** (ChatGPT, Claude in IDE) — produce
   plausible-looking HCL that fails on `terraform plan` because the model
   guessed at attribute names. No version-pinning, no audit log, no
   reproducibility.
3. **Cloud vendor lift-and-shift tools** — cover the trivial 20% of
   resources, fall over on anything stateful, and lock you into the
   destination vendor.

**Terrashift's niche:** a tool that *knows* the schemas (RAG over
version-pinned Terraform Registry data), runs idempotently, keeps a
signed audit log, and uses LLMs as a *fallback* only when the
deterministic path doesn't have an answer.

The three goals (Constitution Article I + supporting articles):

1. **Reproducible** — the same migration today produces the same output
   six months from now. Schemas pinned per migration. Audit log with
   chained hashes. Idempotency keys on every state-mutating operation.
2. **Cheap to run** — 5–10× cheaper per migration than raw LLM use.
   Multi-tier caching (in-memory → SQLite → vector store → cold call).
   Tiered model routing.
3. **Safe** — no long-lived credentials in process memory. Sandboxed
   `terraform apply`. Secret substitution at the LLM boundary. Signed
   audit trail. The kind of safe that passes compliance review.

---

## 3. The thesis: deterministic-first, LLM where it earns its place

This is the most important architectural decision in the project. Read
it twice.

A Terraform migration is **mechanical translation 90% of the time**:
copy the resource, swap `aws_vpc` for `azurerm_virtual_network`, retarget
the attributes, emit. Only the remaining 10% — equivalence ambiguity,
post-apply failures, cost trade-offs — actually benefits from LLM
reasoning.

Of the **nine pipeline components**, only **three are agents** in the
LLM-loop sense:

| Component | Mode | Why |
|---|---|---|
| Scanner | Deterministic Rust | HCL parsing with `hcl-rs`. No LLM. |
| **Mapper** | LLM, structured output (1 call) | Cross-cloud equivalence (`aws_vpc` ↔ `azurerm_virtual_network`). One JSON-typed call, then deterministic code. |
| **Planner** | LLM, structured output (1 call) | Migration order DAG. One call, then deterministic. |
| Generator | Deterministic Rust + templates | Emits HCL from cached templates. LLM fills gaps in Stage 5. |
| Validator | Deterministic Rust | Live provider schema check. Hallucinations caught here. No LLM. |
| Executor | Deterministic Rust | Sandboxed `terraform apply`. No LLM. |
| Verifier | Deterministic Rust | Post-apply state diff. No LLM. |
| **Recovery** | **Agent** (Stage 2) | Failures need adaptation. LLM loop with tool access. |
| **Cost Optimizer** | **Agent** (Stage 2) | Cost trade-offs benefit from reasoning. LLM loop. |

**Mapper and Planner are LLM-with-structured-output, not agents.** They
make exactly one LLM call producing JSON conforming to a schema; then
deterministic Rust takes over. They don't loop. This distinction is
precise and load-bearing.

**Stage 1 ships with zero agents.** Recovery and Cost Optimizer arrive in
Stage 2. This forces us to find out whether the deterministic pipeline
actually works before adding agentic complexity. (Constitution Article I.)

---

## 4. The constitution at a glance

The full text lives at `docs/governance/CONSTITUTION.md`. Read it once, end to end. It's
short (130 lines) and dense. The thirteen articles:

| # | Article | One-line summary |
|---|---|---|
| I | Architectural restraint | Of nine pipeline components only three are agents. New agent ⇒ RFC + stage gate. |
| II | Reference codebase discipline | Stakpak (`refs/stakpak/`) is the primary architectural reference; cite `stakpak_arch.md` section numbers in PRs. |
| III | AI safety | Every LLM-emitted attribute is checked against the live provider schema before HCL emit. Hallucinations are a build break, not a warning. |
| IV | Failure mode handling | Loud failures. No silent fallbacks. No swallowed errors. |
| V | Credentials & security | No long-lived creds in process memory. LLM never sees raw secrets. Every credential operation audited. |
| VI | Knowledge layer integrity | Schemas version-pinned per migration. No "latest" anywhere in production paths. |
| VII | Repository hygiene | Trunk-based dev. PRs ≤500 lines preferred. Clippy lints `unwrap_used`, `expect_used`, `string_slice` are workspace-deny. |
| VIII | Stage gates | Stages don't progress without exit-gate sign-off. Adding agentic complexity in Stage 2 must not increase happy-path Stage-1 cost by >1.3×. |
| IX | Data governance | State files, audit logs, migration outputs are user property. Archival, never deletion. |
| X | Observability | Every LLM call, every tool invocation, every pipeline stage gets a `tracing` span. Structured logs. No `println!` in production. |
| XI | Amendments | Constitution amended by RFC. Versioned. |
| XII | Token economy | Cache-first. Tiered routing. CI regression gate (>30% cost increase blocks merge). Top-10 prompt audit monthly. |
| XIII | Source-derived anti-patterns | Ten rules lifted from Stakpak's hard-won lessons. The concrete checklist behind I–XII. |

**Article XIII rules you'll bump into within your first week:**

- **Rule 1** — Don't bypass the message conversion pipeline. Skipping
  `ContextReducer::reduce` and feeding raw messages produces Anthropic
  400s on dangling `tool_use`.
- **Rule 2** — Don't make trim boundaries non-monotonic. If
  `trimmed_up_to_message_index` ever goes backward, every Anthropic
  prompt-cache hit dies — kills Article XII rule 2.
- **Rule 3** — No `unwrap()` / `expect()` / `&s[..n]` outside tests.
  Workspace-level Clippy denies these. Use `?`, `.get(..)`, `match` with
  `MapperLookupError::Missing`.
- **Rule 5** — Don't bypass the proxy redaction layer. Tool results must
  go through `redact_content` before reaching the LLM.

---

## 5. Architecture in one diagram

```
┌──────────────────────────────────────────────────────────────────┐
│  CLI / TUI (Ratatui)                                             │
│     terrashift migrate ...   │   terrashift (no-args → TUI)     │
├──────────────────────────────────────────────────────────────────┤
│  Migration Engine (the brain)                                    │
│                                                                  │
│   Scanner ──────► Mapper ──────► Planner ──────► Generator       │
│   (det. HCL)     (1 LLM call)   (1 LLM call)   (det. + tmpl.)    │
│                                                                  │
│            └─► Validator ──► Executor ──► Verifier ─────────►    │
│                (schema)      (tf apply)    (state diff)          │
│                                                                  │
│   Stage 2:  Recovery (agent)  •  Cost Optimizer (agent)          │
├──────────────────────────────────────────────────────────────────┤
│  Foundation                                                      │
│   Knowledge service (schemas, mappings, vector store)            │
│   LLM client (stakai)                                            │
│   MCP layer (rmcp, mTLS) — Stage 5+                              │
│   Audit log (chained Ed25519 signatures)                         │
│   Credential broker (no long-lived creds)                        │
└──────────────────────────────────────────────────────────────────┘
```

The Knowledge service is your domain. The LLM client and the Mapper/Planner
prompts are your domain. The Validator gate (Article III) is the wall
between LLM hallucinations and disk; you'll harden it in Stage 2+.

---

## 6. Repository layout

Terrashift is a Cargo **workspace** with 16 member crates and one
binary (`terrashift`, built from `cli/`). It's modeled on Stakpak's
workspace (~28 crates) but slimmer because we drop the gateway,
autopilot, and managed-service code. The TUI is a library that the
CLI links against — `use terrashift_tui::start_tui` from
`cli/src/main.rs`.

```
terrashift/
├── cli/                           # The `terrashift` binary
│   ├── src/main.rs                # Entry: subcommand dispatch
│   ├── src/commands/              # migrate, scan, schemas, util
│   └── tests/cli_smoke_test.rs    # Subprocess tests against the binary
│
├── tui/                           # Interactive TUI (ratatui)
│   ├── src/app.rs                 # AppState (input buffer, scroll, history)
│   ├── src/view.rs                # Single-frame ratatui rendering
│   ├── src/event_loop.rs          # start_tui() entry + key handling
│   ├── src/commands/              # Per-file slash commands (Claude Code style)
│   └── tests/                     # 36 tests across app/view/dispatch
│
├── libs/
│   ├── shared/                    # Shared types (ChatMessage envelope, IDs)
│   ├── agent-core/                # Tool trait, ContextReducer, Compaction
│   ├── ai/                        # stakai-backed LLM client; provider configs
│   ├── knowledge/                 # ★ RAG pipeline — your home turf
│   │   ├── src/types.rs                    # ProviderSchema, ResourceSchema, AttributeSchema
│   │   ├── src/schema_store.rs             # SQLite persistence
│   │   ├── src/vector_store.rs             # In-memory cosine search
│   │   ├── src/embedding.rs                # StubEmbeddingService (deterministic)
│   │   ├── src/registry_client.rs          # SchemaFetcher trait
│   │   ├── src/schema_fetcher_cli.rs       # `terraform providers schema -json` impl
│   │   ├── src/knowledge_service.rs        # KnowledgeService — the facade
│   │   ├── seed/                           # 173 bundled JSON schemas
│   │   │   ├── aws/<category>/aws_*.json   # 142 files
│   │   │   ├── azurerm/<category>/*.json   # 15 files
│   │   │   └── google/<category>/*.json    # 16 files
│   │   └── tests/                          # 14 tests incl. seed integrity
│   │
│   ├── engine/                    # The pipeline
│   │   ├── src/scanner/           # HCL → EstateInventory
│   │   ├── src/mapper/            # EstateInventory → MappingPlan (1 LLM call)
│   │   ├── src/validator/         # Schema validation
│   │   ├── src/generator/         # MappingPlan → .tf files (templates)
│   │   ├── src/executor/          # Sandboxed terraform apply
│   │   ├── src/verifier/          # Post-apply state diff
│   │   ├── src/planner/           # (S4) DAG ordering
│   │   ├── src/recovery/          # (S10) Recovery agent
│   │   └── src/cost_optimizer/    # (S11) Cost Optimizer agent
│   │
│   ├── audit/                     # Append-only chained audit log
│   ├── creds/                     # Credential broker
│   ├── eval/                      # Golden-migration eval harness
│   ├── notify/                    # User-facing notifications
│   ├── shell-tool-approvals/      # Tree-sitter approval for run_command
│   └── mcp/{client,server,proxy,config}/  # MCP layer (Stage 5+)
│
├── fixtures/
│   └── aws-to-azure-real/         # PratikMahajan AWS-to-Azure migration repo
│       ├── aws/modules/vpc/       # The end-to-end test harness scans this
│       └── ...
│
├── refs/                          # Reference codebases (NOT committed)
│   ├── stakpak/                   # Fresh clone of stakpak/agent
│   ├── claude-code/               # Junction to ClaudeCode-CLI-Src
│   └── stakpak_arch.md            # ~2,840-line architectural reference
│
├── specs/                         # Spec Kit work products
│   └── 0NN-feature-name/{spec,clarify,plan,tasks,analyze,checklist}.md
│
├── docs/
│   ├── onboarding/this-file.md
│   └── speckit_commands.txt
│
├── .specify/                      # Spec Kit templates & state
├── .claude/                       # Claude Code agents, skills, slash commands
├── CLAUDE.md                      # Operating instructions for Claude Code
└── docs/                          # All written material
    ├── README.md                  # ← start here for the docs index
    ├── governance/                # The rules
    │   ├── CONSTITUTION.md        # ← read this end-to-end
    │   ├── pre-flight.md          # Environment-specific decisions (paths, OS)
    │   └── SESSION_PLAN.md        # Multi-session roadmap
    ├── architecture/              # What we're building
    │   ├── terrashift_plan.md     # ← skim for context
    │   └── TERRASHIFT_MAPPING.md  # Stakpak seam ↔ Terrashift mapping (P-00)
    ├── development/               # How we build it
    │   ├── terrashift_prompts.md  # Implementation prompts library
    │   └── speckit_commands.txt   # Spec Kit slash-command cheat sheet
    ├── onboarding/                # ← this folder
    │   ├── terrashift-mvp-onboarding.md
    │   ├── CLAUDE-HANDOVER.md
    │   └── terrashift_opus_setup.md
    └── reference/
        └── stakpak_arch.md        # ~2,840-line architectural analysis
```

**Crate dependency rule:** `cli` and `tui` depend on `libs/*`; libs/* don't
depend on cli or tui. `libs/engine` depends on `libs/{ai, knowledge,
shared, agent-core}` and nothing else. This keeps the engine
embeddable in non-CLI contexts (e.g., a future SaaS surface).

---

## 7. The migration pipeline — component-by-component

This is what runs when an operator types
`terrashift migrate --source ./aws-prod --from aws --to azurerm`.

### 7.1 Scanner (`libs/engine/src/scanner/`)

**What it does:** walks the source directory recursively, parses every
`.tf` file with `hcl-rs`, returns an `EstateInventory` of resources, data
sources, providers, modules, variables, and outputs.

**What it doesn't do:** no LLM, no network, no schema lookups. Pure
parsing. Hard-fails on `dynamic` blocks (Article IV — Stage 1 doesn't
support them; Stage 5 will).

**Public API:**

```rust
let inventory = Scanner::scan(Path::new("./aws-prod"))?;
println!("{} resources", inventory.resource_count());
```

**Tested with:** 12 unit + integration tests including the real
PratikMahajan AWS VPC fixture, broken HCL → `Parse` error, dynamic blocks
→ `UnsupportedFeature`, empty dirs, hidden-dir skip rules.

### 7.2 Mapper (`libs/engine/src/mapper/`)

**What it does:** takes the `EstateInventory` and the source/target
provider, makes **exactly one** LLM call producing a JSON `MappingPlan`
that lists every source resource and its target equivalent.

**Why it's not an agent:** one call, structured output, then deterministic
code. No looping. Article I.

**The flow:**

```
EstateInventory
   │
   ▼
ContextReducer::reduce  ← Article XIII rule 1 (always on the path)
   │
   ▼
KnowledgeService.find_similar_in_provider(target, top_k=5)
   │  ← per source resource type, semantic search the seeded vectors
   │
   ▼
prompt assembly (system + user + RAG hits + canonical-JSON inventory)
   │
   ▼
MapperCache.get(sha256(canonical-json))   ← Article XII rule 2
   │
   ├─ hit ──► return cached MappingPlan
   │
   ▼ miss
LlmClient.complete(...)
   │
   ▼
parse JSON → MappingPlan { run_id, source_provider, target_provider, resources: [MappedResource] }
   │
   ▼
MapperCache.put(...)
```

**This is your hot zone.** The retrieval quality (`find_similar_in_provider`)
directly determines mapping quality. The prompt template
(`libs/engine/src/mapper/prompt.rs`) directly determines output structure.
The schemas you embed determine whether the LLM picks
`azurerm_subnet_route_table_association` (correct) vs.
`azurerm_subnet_network_security_group_association` (wrong).

### 7.3 Planner (`libs/engine/src/planner/`)

**What it does:** consumes the `MappingPlan` and produces a dependency
DAG so resources are emitted/applied in topological order. Stage 1 ships
a deterministic topo-sort over `dependencies: Vec<String>` from each
`MappedResource`. Future LLM-augmented planner is S4+.

### 7.4 Generator (`libs/engine/src/generator/`)

**What it does:** consumes the `MappingPlan`, looks up a per-`target_type`
template from `TemplateRegistry`, calls it with the `MappedResource`,
emits HCL via `hcl-rs`. One file per target type
(`azurerm_virtual_network.tf`, `azurerm_subnet.tf`, …) grouped under the
`--output` directory.

**17 templates registered today.** Hard-coded list:

```
aws_vpc, aws_subnet, aws_security_group, aws_instance, aws_s3_bucket,
azurerm_virtual_network, azurerm_subnet, azurerm_network_security_group,
azurerm_linux_virtual_machine, azurerm_storage_account,
aws_iam_role, google_storage_bucket,
azurerm_route_table, azurerm_public_ip,
azurerm_subnet_network_security_group_association,
azurerm_subnet_route_table_association,
azurerm_resource_group
```

**Template miss is loud.** A `MappedResource` with a `target_type` not in
the registry → `GeneratorError::TemplateMiss`. The Recovery agent (S10)
will close template gaps via LLM in Stage 2; until then, missing
templates are loud failures (Article IV).

**Backup-first.** Pre-existing `<output>/<target>.tf` is moved to
`.terrashift/runs/<run_id>/backups/<op_uuid>/` before overwrite. Rollback
restores byte-identical content. (Article IX — archival, never deletion.)

**Determinism.** Same `MappingPlan` twice → byte-identical output.
`BTreeMap` everywhere, sorted iteration, no `HashMap` in serialization
paths. (Article VI.)

### 7.5 Validator (`libs/engine/src/validator/`)

**What it does:** before `MappingPlan` reaches the Generator, every
attribute on every `MappedResource` is checked against the live provider
schema fetched from the Knowledge service. Hallucinated attributes → loud
error. **The Validator is non-bypassable.** (Article III.)

This is the wall between LLM hallucinations and disk. If the Mapper
emits `azurerm_storage_account.bucket = "x"` (which doesn't exist —
the attribute is `name`), the Validator catches it before any HCL gets
written.

### 7.6 Executor (`libs/engine/src/executor/`)

**What it does:** runs `terraform plan` and (with explicit operator
approval) `terraform apply` against the generated tree. Sandboxed —
Stage 2+ runs this in a container; Stage 1 runs it locally with
operator confirmation.

**Approval model:** tree-sitter command-level approval per Article XIII
rule 6. `terraform apply -auto-approve` is *never* approved wholesale;
every state-mutating call requires explicit operator OK.

### 7.7 Verifier (`libs/engine/src/verifier/`)

**What it does:** post-apply, diffs the new state file against the
expected state. Surfaces drift, missing resources, unexpected creations.

### 7.8 Recovery (`libs/engine/src/recovery/`) — Stage 2

**What it does:** when the deterministic pipeline fails (Validator
rejects a mapping, Executor's `terraform plan` errors, Verifier finds
drift), Recovery is the LLM agent that loops with tool access until the
failure is resolved or the operator escalates.

**Status:** structural code exists; not wired into the runtime. Wiring
this is on the post-MVP backlog.

### 7.9 Cost Optimizer (`libs/engine/src/cost_optimizer/`) — Stage 2

**What it does:** queries Infracost before any `apply`, compares
before/after, surfaces unexpected cost shifts, proposes alternatives.

**Status:** structural code exists; real Infracost adapter not wired.

---

## 8. The knowledge layer (RAG) — your home turf

This is where ML expertise will move the needle most. Read this section
twice.

### 8.1 The shape of the problem

The Mapper needs to answer: "what's the Azure equivalent of `aws_vpc`?"
There are ~1,400 AWS resources, ~900 Azure resources, ~600 GCP resources.
**You cannot fit all 2,900 schemas into an LLM context window** and
asking the LLM to list candidates from memory hallucinates 30%+ of the
time.

The solution: **retrieve a small candidate set from a vector index**,
hand only those candidates to the LLM, and validate the LLM's choice
against the canonical schema before emitting HCL.

### 8.2 The three-tier knowledge cache

Every layer fails independently:

```
Tier 1: Bundled seed
    libs/knowledge/seed/<provider>/<category>/<resource>.json
    173 files: aws=142, azurerm=15, google=16
    Loaded on every cold start, near-instant, offline-capable.
    Stamped with version "terrashift-seed-2025.01" in the schema store.

Tier 2: Terraform Registry pull (on first launch)
    `terrashift schemas sync --provider aws --version 5.30.0`
    Subprocesses `terraform init` + `terraform providers schema -json`
    in a tempdir, parses ~1,300 resource schemas, embeds, persists.
    First launch fetches AWS + Azure automatically (~30s each).
    Stamped with the real provider version in the schema store.

Tier 3: Vector retrieval at query time
    KnowledgeService.find_similar_in_provider(query, target, top_k=5)
    embeds the query → cosine search → top-K hits with full schemas.
    Mapper passes ONLY those to the LLM, never the full universe.
```

**Why two stamped versions?** The seed is curated (covers the demo
paths and common patterns). The registry pull is bigger and more
current. Both live alongside in the schema store keyed by `(provider,
version)`. Mapper retrieval queries by `provider` only, so both
versions surface in candidate hits and the bigger / more-current one
wins on coverage.

### 8.3 The KnowledgeService API

```rust
// Construction
let svc = KnowledgeService::new(
    schema_store: Arc<dyn SchemaStore>,      // SQLite (or in-memory)
    vector_store: Arc<dyn VectorStore>,      // In-memory cosine
    embedding: Arc<dyn EmbeddingService>,    // StubEmbeddingService (today)
    fetcher: Arc<dyn SchemaFetcher>,         // TerraformCliSchemaFetcher (today)
);

// First-launch boot (called by the CLI on cold start)
let report = svc.first_launch_sync(
    &seed_dir,
    &[("hashicorp".into(), "aws".into(), "5.30.0".into()),
      ("hashicorp".into(), "azurerm".into(), "3.50.0".into())],
).await?;

// Mapper-facing retrieval
let hits = svc.find_similar_in_provider(
    "vpc network virtual private cloud",  // query
    "azurerm",                             // target provider filter
    5,                                     // top-K
).await?;
// hits: Vec<ResourceMatch> with full ResourceSchema attached

// Direct lookup (when Mapper already knows the type)
let schema = svc.fetch_schema("azurerm", "3.50.0").await?;
```

### 8.4 The current embedding service is a stub

`StubEmbeddingService` is **deterministic but content-agnostic** — it
hashes the input string into a fixed-dimensional vector. This is enough
to verify the pipeline plumbing (the four tests in
`libs/knowledge/tests/seed_bundle_integrity_test.rs` confirm this end-to-end),
but it gives zero semantic similarity. Two unrelated resources will
have orthogonal vectors; two synonyms will have orthogonal vectors.

**Swapping this for a real embedding model is one of the highest-leverage
changes you can make in your first two weeks.** See section 14.1 below.

### 8.5 The seed bundle layout

Each seed file is one `ResourceSchema`:

```json
{
  "name": "aws_instance",
  "description": "Virtual server in the AWS cloud for running applications",
  "attributes": {
    "ami": {
      "name": "ami",
      "attribute_type": "string",
      "required": true,
      "optional": false,
      "computed": false,
      "sensitive": false,
      "deprecated": null,
      "description": "AMI ID to use for the instance"
    },
    "instance_type": { "name": "instance_type", "required": true, ... }
  }
}
```

**Refresh path:** `libs/knowledge/seed/scripts/import-resource-catalog.mjs`
(standalone Node script, fs-only) regenerates the bundle from a TS-shaped
upstream (we converted CloudForge's resource catalog this way). The
script extracts `terraform_resource`, `description`, `inputs.{required,optional}`
from each TS file, and writes the JSON envelope.

---

## 9. LLM integration

### 9.1 stakai is the LLM SDK

`stakai` (Stakpak's Apache-2.0 LLM crate) is provider-agnostic:
OpenAI, Anthropic, Bedrock, Gemini, plus anything OpenAI-compatible.
Terrashift's `libs/ai` is a thin wrapper that routes calls through
stakai with our profile-resolved endpoint and model.

```rust
use terrashift_ai::{LlmClient, RealClient, Resolver, Tier};

let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco)?;
let client = RealClient::new(resolved)?;
let response = client.complete(&messages, &options).await?;
```

### 9.2 OpenAI-compatible providers (HuggingFace, Vodafone, etc.)

When the operator's profile points at a non-stock endpoint, we configure
stakai's `InferenceConfig::openai(api_key, Some(api_endpoint))`. This is
how we route to HuggingFace's Inference Providers (router.huggingface.co/<provider>/v1)
to use Llama-3.3-70B-Instruct-Turbo via the Together backend, for example.

There's an important quirk: **stakai's provider registry only knows
"openai", "anthropic", "gemini", "bedrock"**. It doesn't recognize
"huggingface" as a provider. We work around this by always passing
`"openai"` as stakai's provider name regardless of what the operator's
config block calls the provider; the operator's `provider_key`
(e.g., `"huggingface-together"`) becomes a Terrashift-side label only.
See `libs/ai/src/providers/openai_compat/provider.rs`.

### 9.3 Tiered routing

Two tiers in production:

- **eco** — single-call, structured-output components (Mapper, Planner).
  Default to the cheapest capable model the customer has configured
  (e.g., Haiku 4.5 or Llama-3.3-70B).
- **smart** — bounded ReAct loops needing reasoning (Recovery, Cost
  Optimizer). Default to a frontier model (Sonnet 4.6 or Opus 4.7).

The Validator is deterministic and uses no LLM (no tier).

### 9.4 The profile config

```toml
# ~/.terrashift/profile.toml
[profile]
default_model = "huggingface-together/meta-llama/Llama-3.3-70B-Instruct-Turbo"

[tiers]
eco = "huggingface-together/meta-llama/Llama-3.3-70B-Instruct-Turbo"
smart = "anthropic/claude-sonnet-4-6"

[providers.huggingface-together]
type = "openai-compatible"
api_endpoint = "https://router.huggingface.co/together/v1"
api_key_env = "HF_API_KEY"

[providers.anthropic]
type = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"
```

### 9.5 Where prompts live

`libs/engine/src/mapper/prompt.rs` — the Mapper's system + user prompts.
**This is your prompt-engineering surface.** When you change retrieval
behaviour, you'll often want to change the prompt template too (e.g.,
to make the LLM cite which RAG hit it picked).

---

## 10. The CLI surface

`terrashift` is the single binary. Subcommands:

### `terrashift` (no args)

Opens the interactive ratatui TUI. Mirrors the UX of `claude` and
`stakpak`. See section 11.

### `terrashift version`

Prints version + Rust toolchain.

### `terrashift migrate`

The full pipeline:

```powershell
terrashift migrate `
    --source ./aws-prod `
    --from aws `
    --to azurerm `
    --output ./azure-prod `
    --tier eco
```

Flow: load profile → build LLM client → build KnowledgeService with seed
loaded → Scanner → Mapper (real LLM) → Validator → Generator (best-effort
per-resource emission). `--dry-run` skips the Generator (plan-only).

### `terrashift scan <dir>`

Read-only inventory dump. No LLM, no network. Useful as a sanity check
before `migrate`.

### `terrashift schemas sync --provider aws --version 5.30.0`

Pulls a provider schema from the Terraform Registry via subprocess
`terraform init` + `terraform providers schema -json`. Caches it in
`~/.terrashift/cache/schemas.sqlite`. Embeds + indexes the resources.

### `terrashift schemas list`

Lists cached `(provider, version)` tuples.

### `terrashift schemas seed`

Loads only the bundled seed (offline; no registry calls). Useful as a
smoke test that the seed bundle is reachable from the installed binary.

### Profile resolution priority

1. `--profile <path>` flag
2. `$TERRASHIFT_PROFILE` env var
3. `~/.terrashift/profile.toml` (or `%USERPROFILE%\.terrashift\profile.toml`
   on Windows)

### Error format

Recoverable errors print a clean Display chain, exit 1:

```
error: scan C:\does\not\exist
  caused by: io error at C:\does\not\exist: scan root does not exist: C:\does\not\exist
  caused by: scan root does not exist: C:\does\not\exist
```

**No Rust backtraces leak to users.** This was a real bug fixed during
MVP testing — `cli/tests/cli_smoke_test.rs::scan_nonexistent_dir_prints_clean_error_no_backtrace`
is the regression test that pins this behaviour.

---

## 11. The interactive TUI

Open it with bare `terrashift`. You get:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ 🚀 Terrashift  v0.1.0  •  cross-cloud Terraform migration              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│ Welcome. Try /help to list commands, /scan <dir> to inspect a tree.    │
│                                                                         │
│ For full migrations with real LLM, run `terrashift migrate ...` from   │
│ the shell (the in-TUI agent runtime arrives in S6).                    │
│                                                                         │
│ Press Ctrl+C or type /quit to exit.                                    │
│                                                                         │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│ ▶                                                                       │
└─────────────────────────────────────────────────────────────────────────┘
  profile: ~/.terrashift/profile.toml │ seed: 173 resources │ tier: eco │ Ctrl+C to exit
```

### Slash commands (11 registered)

| Command | Status | What it does |
|---|---|---|
| `/help` | ✅ live | Lists commands with descriptions |
| `/scan <dir>` | ✅ live | Real Scanner call; returns inventory in chat |
| `/audit` | 🟡 stub | S6 — show audit log tail |
| `/migrate <plan>` | 🟡 stub | S6 — runs pipeline interactively (today: redirects to shell) |
| `/checkpoint` | 🟡 stub | S6 — checkpoint the current run |
| `/plan` | 🟡 stub | S2+ — show migration plan |
| `/cost` | 🟡 stub | S2+ — show cost analysis |
| `/rollback <run_id>` | 🟡 stub | S6 — rolls back a run |
| `/compact` | 🟡 stub | S6 — compacts message history |
| `/quit` / `/exit` | ✅ live | Graceful shutdown |

### Architecture

- `tui/src/app.rs` — `AppState` (message history, input buffer, cursor,
  scroll offset, status footer). Pure data; no I/O.
- `tui/src/view.rs` — single `view()` fn that renders every frame.
  Layout: 3-row banner, flex messages, 3-row input, 1-row status.
- `tui/src/event_loop.rs` — `start_tui()` mainloop. Raw mode + alt
  screen + 100ms event poll. `TerminalGuard` impl `Drop` for
  panic-safe cleanup.
- `tui/src/commands/` — per-file slash commands (Claude Code pattern,
  borrowed from `refs/claude-code/src/commands/`). One Rust file per
  command keeps add-a-command at 1 file + 1 registry line.

### Tests

- `tests/app_state_test.rs` (14) — input editing, UTF-8 (café, 🚀),
  scroll bounds, take_input semantics.
- `tests/view_render_test.rs` (7) — drives `ratatui::TestBackend` at
  120×30 / tiny 20×5 / long history / unicode / absurd scroll_offset.
  All render without panic.
- `tests/slash_commands_test.rs` (15) — registry size, help lists every
  command, unknown command friendly error, /scan with real fixture, etc.

---

## 12. Test landscape & MVP scorecard

**278 tests passing across 54 suites, 0 failures.** Some `#[ignore]`'d
because they require network or external services (Terraform CLI,
real LLM API key).

### Per-component coverage

| Layer | Tests | Highlights |
|---|---|---|
| Scanner | 12 | Real PratikMahajan VPC fixture, broken HCL, dynamic blocks, hidden-dir skip |
| Mapper | 11 | Cache key stability, prompt assembly, structured-output parse |
| Validator | 18 | Schema mismatch, missing required, extra hallucinated attrs |
| Generator | 14 | All 17 templates × {minimal-valid, missing-required loud-fail}, round-trip with Scanner, byte-identical determinism, backup-first, rollback |
| Executor | 8 | Plan-only mode, sandbox enforcement, approval gate |
| Knowledge | 14 (incl. seed integrity) | All 173 JSONs parse, counts match filesystem, idempotent reseed, find_similar_in_provider filters |
| Audit | 12 | Chained hashes, replay verification, redaction at write time |
| Creds | 9 | STS resolution, token zeroization, audit on every fetch |
| AI | 10 | stakai client wiring, tier resolution, profile parsing |
| TUI | 36 | AppState UTF-8, view non-panic across edge cases, 11 slash commands |
| CLI | 9 | Subprocess tests (--help, version, missing args, no-backtrace regression, real fixture scan) |
| Eval | 6 | Golden-migration scoring, token-cost regression gate |

### What's `#[ignore]`'d (and why)

- `libs/knowledge/src/schema_fetcher_cli.rs::test_real_aws_fetch` — needs
  `terraform` on PATH, ~30s. Verified manually: returns 1300 resources +
  533 data sources for `aws@5.30.0`.
- `libs/engine/tests/pratik_e2e_test.rs::pratik_aws_to_azure_e2e` — needs
  `HF_API_KEY` and a live Llama-3.3 endpoint. Verified manually: 9
  resources mapped, 1 .tf file emitted (the 6 skipped resources are
  cases where the LLM omitted required attributes; Recovery agent S10
  closes that gap).

### MVP-ready verdict

✅ **Yes, with two known caveats:**

1. **Network-bound tests are gated by env-var presence.** Correct CI
   hygiene, not a gap.
2. **Generator coverage is intentionally narrow** — 17 templates today.
   A migration to a target type *not* in the registry → loud
   `TemplateMiss` error. Recovery agent (S10) closes that gap via LLM
   in Stage 2. Until S10, operators see clear errors instead of
   silently wrong .tf.

The full per-claim scorecard is in the latest `git log` entry —
commit `a38eaf8` ("test: deeper MVP coverage").

---

## 13. Stage roadmap

### Stage 1 — Deterministic core ✅ shipped

- Scanner, Mapper (1 LLM call), Validator, Generator, Executor, Verifier
- Knowledge service with three-tier cache
- 17 Generator templates
- CLI subcommands + interactive TUI
- 278-test workspace, 0 failures

### Stage 2 — Two agents, real cost integration 🔜 next up

Article VIII: "adding agentic complexity in Stage 2 must not increase
happy-path Stage-1 migration cost by more than 1.3×."

Items in the backlog (not yet started):

1. **Recovery agent wiring** — `libs/engine/src/recovery/mod.rs`
   structural code exists; needs to be wired to the runtime so it loops
   on Validator/Executor failures with tool access. ~600 LOC.
2. **Cost Optimizer with real Infracost adapter** — replace the stub
   with `InfracostCostService`. Article IX requires the cost-impact
   audit entry. ~250 LOC + 6 tests. INFRACOST_API_KEY available.
3. **`JsonAgentLlmClient` integration** — adapt the Mapper's structured-
   output JSON parser to the agent loop (Recovery + Cost Optimizer share
   the same client, different prompts). ~150 LOC.

### Stage 3 — Mapping examples corpus

Curated AWS↔Azure↔GCP equivalence corpus, embedded into the vector
store as `MappingExample` rows alongside `ResourceSchema`. The Mapper
retrieves these as few-shot examples in its prompt. Stage 3 is your
arrival window for high-impact contribution.

### Stage 4 — Cutover agent

Stateful migrations (databases with replication windows, DNS cutover with
TTL planning). Third agent. Bounded blast radius.

### Stage 5 — Terraform completeness

`dynamic` blocks, `count`, `for_each`, advanced module composition. Today
these are loud-fail in the Scanner.

### Stage 6 — Managed surfaces

In-TUI agent runtime (slash `/migrate` runs the pipeline interactively
with streaming progress), MCP server for IDE integration, optional SaaS
control plane.

---

## 14. Where your ML expertise plugs in

The four highest-leverage contributions ranked by impact-per-LOC.

### 14.1 FastEmbed swap (~150 LOC + new dep) — **start here**

Today's `StubEmbeddingService` is content-agnostic. A real local
embedding model gives the Mapper meaningful semantic similarity
out-of-the-box.

**The shape:**

```rust
// libs/knowledge/src/embedding.rs (today)
pub trait EmbeddingService: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}

pub struct StubEmbeddingService { ... }   // hashes input → fixed vector

// What you'll add:
pub struct FastEmbedService {
    model: fastembed::TextEmbedding,
}
```

**Crate:** `fastembed-rs` — pure-Rust ONNX runtime, ships embedding
models like `BAAI/bge-small-en-v1.5` (384-dim), no Python, no GPU.
Workspace already targets 384-dim vectors (`VECTOR_DIM` const in
`cli/src/commands/util.rs`).

**Tests to write:**
- Same query embedded twice → identical vector (determinism).
- "vpc network" embedding has higher cosine with `azurerm_virtual_network`
  description than with `azurerm_storage_account` description (semantic
  signal).
- The provider-filtered `find_similar_in_provider` test in
  `seed_bundle_integrity_test.rs` should now show that the *top* hit
  for "storage account blob" is `azurerm_storage_account`, not just
  any azurerm hit.

**Impact:** without this, the Mapper has no semantic grounding —
retrieval is provider-filtered but the rank order is meaningless.

### 14.2 Mapping examples corpus (~500 LOC + curation)

The Mapper's prompt today says: "given source resource `aws_vpc`, here
are 5 candidate target resources from semantic search; pick the best
equivalent." This is zero-shot.

A few-shot prompt with curated examples ("`aws_vpc` ↔
`azurerm_virtual_network` because both define a CIDR block address
space; `address_space` in azurerm is the equivalent of `cidr_block`
in aws") is dramatically better.

**The shape:**

```json
{
  "source_provider": "aws",
  "target_provider": "azurerm",
  "source_type": "aws_vpc",
  "target_type": "azurerm_virtual_network",
  "rationale": "both define a CIDR block address space",
  "attribute_mappings": {
    "cidr_block": "address_space[0]",
    "tags": "tags",
    "enable_dns_hostnames": "(deferred to azurerm_private_dns_zone)"
  }
}
```

`MappingExample` is already a type in `libs/knowledge/src/types.rs` and
`KnowledgeService::search_curated_mappings()` is the lookup API. The
plumbing exists; the *content* is the work — 50–100 hand-curated
examples covering the demo paths (AWS→Azure VPC family, AWS→GCP
storage family, etc.).

### 14.3 Eval harness for Mapper retrieval quality (~300 LOC)

Today the Mapper's correctness is verified manually (the PratikMahajan
e2e test). What you want: a programmatic metric.

**The shape:**

```rust
// libs/eval/tests/mapper_retrieval_quality.rs
struct GoldStandard {
    source_type: &'static str,
    target_provider: &'static str,
    expected_target_type: &'static str,
    acceptable_alternatives: &'static [&'static str],
}

const GOLD: &[GoldStandard] = &[
    GoldStandard {
        source_type: "aws_vpc",
        target_provider: "azurerm",
        expected_target_type: "azurerm_virtual_network",
        acceptable_alternatives: &[],
    },
    // ~50 entries
];

#[tokio::test]
async fn retrieval_recall_at_5_above_threshold() {
    let svc = build_real_knowledge_service().await;
    let mut hits = 0;
    for case in GOLD {
        let candidates = svc.find_similar_in_provider(
            case.source_type,
            case.target_provider,
            5,
        ).await.unwrap();
        if candidates.iter().any(|c|
            c.resource_type == case.expected_target_type ||
            case.acceptable_alternatives.contains(&c.resource_type.as_str())
        ) {
            hits += 1;
        }
    }
    let recall = hits as f64 / GOLD.len() as f64;
    assert!(recall >= 0.85, "recall@5 = {recall:.2}, expected >= 0.85");
}
```

This makes retrieval quality a CI signal. Article XII rule 4 already
requires the eval framework to track token cost; this extends it to
retrieval correctness.

### 14.4 Hard-negative mining for the Validator

Today the Validator catches hallucinated attributes by structural
comparison ("`bucket` doesn't exist on `azurerm_storage_account`,
reject"). It can't catch *plausible-looking but semantically wrong*
mappings (e.g., the LLM picks `azurerm_subnet_network_security_group_association`
when it should pick `azurerm_subnet_route_table_association` — both
exist, both have the right attribute *shape*, but the second is
correct).

A hard-negatives corpus (LLM-misclassified pairs from prior runs)
mined from the audit log → fed back as Validator-level rejection
prompts → catches semantic mis-mapping. This is the highest-impact
ML contribution but the longest lead time (needs production audit log
data).

---

## 15. Onboarding checklist (Day 1 → Week 1)

### Day 1 — Get it running

```powershell
# 1. Clone
git clone <repo-url> C:\Users\<you>\Desktop\terrashift
cd C:\Users\<you>\Desktop\terrashift

# 2. Setup reference codebases (one-time)
.\scripts\setup-refs.ps1

# 3. Build (~5 min cold)
cargo build --release

# 4. Run the test suite (~30s)
cargo test --workspace --no-fail-fast

# 5. Smoke-test the binary
.\target\release\terrashift.exe version
.\target\release\terrashift.exe schemas list
.\target\release\terrashift.exe scan .\fixtures\aws-to-azure-real\aws\modules\vpc

# 6. Open the TUI
.\target\release\terrashift.exe
# (try /help, /scan .\fixtures\..., /quit)
```

If the build fails on `terraform-tools` or other MCP plugins, those are
optional — they don't block the core build. The workspace itself should
build cleanly with stable Rust 1.94+.

### Day 2 — Read the canonicals

In this order:

1. `docs/governance/CONSTITUTION.md` — 130 lines, 13 articles. Read end-to-end.
2. `docs/architecture/terrashift_plan.md` — sections 1–9 (architecture +
   knowledge layer). Skim everything else.
3. `docs/architecture/TERRASHIFT_MAPPING.md` — section A (the 11 seams) +
   section D (top anti-patterns).
4. `docs/governance/pre-flight.md` — Decision 2 (refs/ layout) + Decision 10
   (PratikMahajan fixture).
5. This doc, sections 7 and 8 (pipeline + RAG).

### Day 3 — Trace one full request

Pick `terrashift scan .\fixtures\aws-to-azure-real\aws\modules\vpc` and
trace it end-to-end:

1. `cli/src/main.rs::main` → `commands::scan::run`
2. `cli/src/commands/scan.rs::run` → `Scanner::scan`
3. `libs/engine/src/scanner/mod.rs::Scanner::scan` → `walker::discover_tf_files`
4. `libs/engine/src/scanner/walker.rs` → walks the directory
5. `libs/engine/src/scanner/parser.rs::parse_file` → calls `hcl::from_str`
6. Back to `scan::run` → prints inventory

Then do the same for `terrashift migrate` (much longer — it'll take you
to the Mapper, the Knowledge service, and the Generator).

### Day 4-5 — First contribution

The FastEmbed swap (section 14.1) is sized for a 5-day first PR:

- Day 4: read `libs/knowledge/src/embedding.rs`, the existing trait,
  the four call sites. Write a draft `FastEmbedService` impl.
- Day 5: wire it into `cli/src/commands/util.rs::build_knowledge_service`,
  add tests for determinism + semantic signal, run the pratik_e2e
  manually with real Llama to compare retrieval before/after.

### Week 2 — Second contribution

Pick whichever of section 14.2/14.3/14.4 is most interesting. Open a
spec via Spec Kit (section 16 below).

---

## 16. How we work — Spec Kit workflow

Every non-trivial feature goes through Spec Kit's 7-step inner loop:

```
/speckit-git-feature   →   create a feature branch + spec dir
/speckit-specify       →   write the user-facing spec
/speckit-clarify       →   answer ambiguities (do not skip)
/speckit-plan          →   technical plan
/speckit-tasks         →   break the plan into tasks
/speckit-implement     →   write the code
/speckit-analyze       →   self-review against spec
/speckit-checklist     →   run pre-PR checklist
/speckit-git-commit    →   commit + push
```

Each step writes a markdown file under `specs/<NNN>-<feature>/`. The
clarify step is *especially* non-skippable — most ambiguities surface
there, and resolving them before code is 10× cheaper than after.

Examples in the repo: `specs/000-reference-walkthrough/`,
`specs/004-scanner/`, `specs/007-knowledge-schema-cache/`.

### PR description template

Every PR description has three sections:

```markdown
## Constitution
- Article N (rationale)
- Article XIII rule M (where applicable)

## stakpak_arch.md
- section N (what was mirrored)

## Eval impact
- Token cost delta vs baseline (per Article XII rule 4)
```

---

## 17. Reference materials

### In-repo (read these)

- `docs/README.md` — full docs index
- `docs/governance/CONSTITUTION.md` — the rules
- `docs/architecture/terrashift_plan.md` — what we're building
- `docs/architecture/TERRASHIFT_MAPPING.md` — Stakpak seam mapping
- `docs/governance/pre-flight.md` — environment-specific decisions
- `CLAUDE.md` — operating instructions for AI assistants
- `docs/reference/stakpak_arch.md` — ~2,840-line architectural reference (PRIMARY)
- `refs/stakpak/` — Stakpak source code (Apache 2.0); ground truth (per-developer
  via `scripts/setup-refs.ps1`, gitignored)
- `refs/claude-code/` — Claude Code source (per-developer junction; not
  redistributable)

### External (skim these)

- HashiCorp Terraform Registry — https://registry.terraform.io
- `hcl-rs` — https://github.com/martinohmann/hcl-rs
- `ratatui` — https://ratatui.rs
- `stakai` — https://github.com/stakpak/agent (within `libs/ai`)
- `fastembed-rs` — https://github.com/Anush008/fastembed-rs
- `lancedb` — https://lancedb.com (embedded vector DB; planned for Stage 3)

### Stakpak source-of-truth files (when in doubt)

- `refs/stakpak/libs/agent-core/src/agent.rs` — the agent loop kernel
- `refs/stakpak/libs/ai/src/providers/anthropic/` — provider impl pattern
- `refs/stakpak/libs/api/src/storage.rs` — session storage trait
- `refs/stakpak/libs/mcp/proxy/src/redact.rs` — secret redaction layer

---

## 18. Glossary

| Term | Meaning |
|---|---|
| **HCL** | HashiCorp Configuration Language. The `.tf` file syntax. |
| **EstateInventory** | Output of the Scanner. The full set of resources, providers, modules, etc. discovered in a Terraform tree. |
| **MappingPlan** | Output of the Mapper. Every source resource → target resource mapping with attributes, dependencies, addresses. |
| **MappedResource** | One row in a MappingPlan: source_addr, target_addr, target_type, target_name, attributes, dependencies. |
| **ResourceSchema** | One Terraform resource type's full schema: name, description, attributes (each with type, required/optional, deprecated, description). |
| **ProviderSchema** | A whole provider's schemas: `provider`, `version`, `resources: BTreeMap<String, ResourceSchema>`, `data_sources`, `fetched_at`. |
| **AttributeValue** | The Mapper's typed attribute value enum: String, Number, Bool, Reference, List, Map. References emit unquoted in HCL. |
| **TemplateMiss** | Generator error when no template exists for a `target_type`. Loud-fails in Stage 1; Recovery agent fills the gap in Stage 2+. |
| **Validator gate** | The non-bypassable check between Mapper and Generator. Article III. |
| **Tier** | `eco` or `smart`. Bound at component level, resolved to a model at runtime. |
| **Stage** | Major project phase. We're at the end of Stage 1 + interactive UX. Stage 2 is the next gate. |
| **Article N rule M** | Constitution citation. E.g., "Article XIII rule 3" = no `unwrap()` in production. |
| **Spec Kit** | The `/speckit-*` command set that drives the inner loop. See section 16. |
| **stakai** | Stakpak's LLM SDK. Provider-agnostic. Apache 2.0. |
| **rmcp** | Rust MCP SDK. Used in `libs/mcp/`. Stage 5+. |
| **First-launch sync** | Boot-time job that loads bundled seed + pulls Terraform Registry schemas for AWS/Azure. ~30s on cold start. |
| **PratikMahajan fixture** | `fixtures/aws-to-azure-real/` — a real AWS-to-Azure migration repo we use as the e2e test target. |

---

## Appendix A — Useful one-liners

```powershell
# Run all tests with summary
cargo test --workspace --no-fail-fast 2>&1 |
    Select-String "test result:"

# Lint workspace (workspace-deny on unwrap_used / expect_used / string_slice)
cargo clippy --workspace --all-targets --no-deps

# Run only the seed-integrity tests
cargo test -p terrashift-knowledge --test seed_bundle_integrity_test

# Run the real LLM e2e (requires HF_API_KEY)
$env:HF_API_KEY="<your key>"
cargo test -p terrashift-engine --test pratik_e2e_test -- --ignored --nocapture

# Count seed JSONs by provider
Get-ChildItem libs\knowledge\seed -Recurse -Filter *.json |
    Group-Object { $_.FullName.Split('\')[-3] } |
    Select-Object Count, Name
```

## Appendix B — Hot files to know by heart

These are the files you'll edit most in your first month. Read them now,
even if you don't change anything.

| File | Why it matters |
|---|---|
| `libs/knowledge/src/knowledge_service.rs` | The RAG facade. Every Mapper call goes through here. |
| `libs/knowledge/src/embedding.rs` | The embedding trait. You'll swap the stub here. |
| `libs/knowledge/src/vector_store.rs` | Cosine search. Today in-memory; LanceDB swap is Stage 3. |
| `libs/engine/src/mapper/prompt.rs` | The Mapper's prompt template. |
| `libs/engine/src/mapper/mod.rs` | The Mapper orchestrator. Cache + reducer + LLM call. |
| `libs/engine/src/generator/templates.rs` | All 17 templates. Read it once to feel the shape. |
| `cli/src/commands/util.rs` | `build_knowledge_service()`, `locate_seed_dir()`, `resolve_profile_path()`. |
| `cli/src/commands/migrate.rs` | The full pipeline orchestration from the CLI. |
| `tui/src/event_loop.rs` | TUI mainloop. Where slash commands dispatch. |

---

**Welcome to the team. Open `docs/governance/CONSTITUTION.md`, then this
doc's section 7, then `cargo test --workspace`. By the end of Day 1, you
should have all 278 tests green and a TUI session open. Ping the team
channel when you hit anything unexpected.**
