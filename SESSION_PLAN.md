# Terrashift — Multi-Session Production Plan

**Maintained:** auto-updated by each session at handoff.
**Last updated:** 2026-05-02 by Session 3 (continuing in same conversation).
**Last commit:** `4545065 feat(p11): Audit log — Ed25519 signed, hash-chained, append-only`.

This document divides the v5 plan (`terrashift_plan.md` §19, ~14 months for a
team of 4-8) into **autonomous Claude Code sessions of 4-8 hours each**. Each
session is self-contained: pre-conditions you set up, work the agent does,
deliverables that get committed, and pre-conditions to set up before the
next session.

If you open this repo cold in 2 weeks, find the section "Next session"
below and start there.

---

## Production-readiness ladder

There's no single "ready for production." Pick the tier that fits your goal:

| Tier | What it means | Sessions needed | Cumulative time |
|---|---|---|---|
| **MVP shippable** | Single cloud-pair (GCP→AWS), agentic recovery loop, deterministic Stage 1 components verified end-to-end against real fixture | 1 → 15 | ~10 weeks elapsed |
| **Production-grade** | Multi-cloud (GCP↔AWS↔Azure), RAG-backed Knowledge service, real customer-grade reproducibility | 1 → 25 | ~18 weeks |
| **GA + SaaS** | Data migration, cutover, structural fidelity for real-world Terraform repos, hardened with SOC2-ready audit posture, optional managed control plane | 1 → 37 | ~14 months |

You can stop at any tier. Each tier has its own exit gate.

---

## Session structure conventions

Every session entry has the same shape so future-you (or a fresh agent) can
execute it without re-deriving context:

```
## S{NN} — {title}
**Stage:** v5 stage number (1-6)
**P-NN prompts covered:** P-XX (link to terrashift_prompts.md)
**Pre-conditions:** what YOU must have done before starting
**User input expected during:** questions the agent will ask + good defaults
**Autonomous work:** what the agent does without input
**Deliverables:** what gets committed (typically: source files + Spec Kit
  artifacts in specs/NNN-foo/ + tests + cargo check/test/clippy/fmt clean)
**Success criteria:** how to know it's done
**Estimated duration:** realistic upper bound
**Hand-off to S{NN+1}:** what to set up before next session
```

Each session ENDS with a commit that updates this file — marking the session
done, recording the commit hash, and listing any unresolved questions for
the next session. That's the load-bearing discipline.

---

## Session ledger (overview)

| # | Title | Stage | Status | Commit | Tier |
|---|---|---|---|---|---|
| 1 | Foundation — D0 + D1 + P-02 + P-04 | 1 | ✅ Done | `f61dc31` | All |
| 2 | Knowledge layer with RAG (P-07 expanded; was: schema cache + audit) | 1 | ✅ Done | `2b48216` | All |
| 3 | Audit log (P-11) — Generator + Eval still pending | 1 | 🟡 Partial | `4545065` | All |
| 3b | Generator (P-08) + Eval framework (P-12) | 1 | ⏳ Next | — | All |
| 4 | LLM call (Groq) + Mapper + Validator | 1 | Blocked: needs `GROQ_API_KEY` | — | All |
| 5 | Sandboxed Executor + Credential broker | 1 | Blocked: needs cloud test creds | — | All |
| 6 | TUI slash commands + Release pipeline | 1 | Planned | — | All |
| 7 | Eval expansion (10 golden migrations) | 1 | Planned | — | All |
| 8 | Stage 1 exit gate (P-16) + remediation | 1 | Planned | — | All |
| 9 | Agent loop kernel (run_agent, approval, retry) | 2 | Planned | — | MVP+ |
| 10 | Recovery agent | 2 | Planned | — | MVP+ |
| 11 | Cost Optimizer agent + Infracost service | 2 | Planned | — | MVP+ |
| 12 | WIF/OIDC cloud auth modernization | 2 | Planned | — | MVP+ |
| 13 | Detached mode + notifications | 2 | Planned | — | MVP+ |
| 14 | Eval expansion + Stage 2 hardening | 2 | Planned | — | MVP+ |
| 15 | Stage 2 exit gate | 2 | Planned | — | MVP+ |
| 16 | Azure provider (source + target) | 3 | Planned | — | Production |
| 17 | RAG production (LanceDB + reranker) | 3 | Planned | — | Production |
| 18 | LLM tier router + provider fallback | 3 | Planned | — | Production |
| 19 | GitHub VCS integration | 3 | Planned | — | Production |
| 20 | All 6 cloud-pair smoke tests | 3 | Planned | — | Production |
| 21 | Stage 3 exit gate | 3 | Planned | — | Production |
| 22 | Data migration service (DMS, Storage Transfer) | 4 | Planned | — | GA |
| 23 | Strategy Selector agent | 4 | Planned | — | GA |
| 24 | Cutover agent + DNS plug-ins | 4 | Planned | — | GA |
| 25 | Secrets migration + parity verification | 4 | Planned | — | GA |
| 26 | Stage 4 exit gate | 4 | Planned | — | GA |
| 27 | HCL constructs (dynamic, count, for_each, locals) | 5 | Planned | — | GA |
| 28 | Module ecosystem + Terragrunt + OpenTofu | 5 | Planned | — | GA |
| 29 | Policy translation + plugin system | 5 | Planned | — | GA |
| 30 | Real customer migration validation | 5 | Planned | — | GA |
| 31 | Stage 5 exit gate | 5 | Planned | — | GA |
| 32 | Quality + reliability + chaos testing | 6 | Planned | — | GA |
| 33 | Sandbox/staging mode + drift detection | 6 | Planned | — | GA |
| 34 | SaaS multi-tenancy + bridge runner | 6 | Planned | — | GA |
| 35 | GitLab + Bitbucket integrations | 6 | Planned | — | GA |
| 36 | Compliance (SOC2, signed audit export, GDPR) | 6 | Planned | — | GA |
| 37 | GA gate + reference customer in production | 6 | Planned | — | GA |

---

## Detail — next 5 sessions (S2 → S6)

These are fully specified so any of them can be picked up cold.

### S2 — Knowledge schema cache + Audit log

**Stage:** 1 | **P-NN:** P-07 + P-11 | **Tier:** All

**Pre-conditions:**
- `cargo check --all-targets` passes (verified in S1).
- No external API keys needed — both components use SQLite locally.

**User input expected during:**
- *(none significant)* — clarify questions answered with constitution defaults.

**Autonomous work:**

1. **P-07 — Knowledge schema cache** (`libs/knowledge/`):
   - `SchemaStore` trait with `fetch_provider_schema(provider, version)`,
     `cache_schema()`, `list_versions()` (mirror Stakpak's SessionStorage shape).
   - `LocalSchemaStore` SQLite-backed impl via `sqlx`.
   - HTTP fetch fallback to `registry.terraform.io` on cache miss.
   - Cache-stable invariant: pinned versions never expire (Article VI).
   - `search_mappings()` returns `Vec::new()` (RAG stub, fills in S17).

2. **P-11 — Audit log** (`libs/audit/`):
   - `AuditEntry` schema with prev_hash + content_hash + Ed25519 signature
     (per `terrashift_plan.md` §6.X).
   - `AuditPayload` enum: `LlmCall`, `ToolExecution`, `PhaseTransition`,
     `CredentialResolution`, `FileOperation`.
   - SQLite append-only store via `sqlx`.
   - `verify_chain()`, `export()`, `query()` public APIs.
   - Pre-write redaction scrubber (gitleaks regex set + entropy filter)
     panics on raw secret detection (Article XIII rule 5 — last line of defence).
   - `AuditWriterHook: AgentHook` — registers `after_tool_execution` to emit
     `ToolExecution` payloads.

**Deliverables:**
- `specs/007-knowledge-schema-cache/` (6 artifacts)
- `specs/011-audit-log/` (6 artifacts)
- `libs/knowledge/src/{schema_store,local_schema_store,registry_client,errors}.rs`
- `libs/audit/src/{entry,store,verify,signer,scrubber,hooks,errors}.rs`
- Tests: `tests/schema_cache_test.rs`, `tests/audit_chain_test.rs` (round-trip
  + tamper detection + redaction panic)
- All 4 build gates green (check + test + clippy + fmt)
- 2 commits: `feat(p07): ...`, `feat(p11): ...`

**Success criteria:**
- `cargo test --workspace` passes
- Audit chain test demonstrates tamper detection (modify any field → verify_chain returns Err)
- Schema cache test demonstrates: first fetch hits registry, second is local-only

**Estimated duration:** 4-5 hours.

**Hand-off to S3:**
- Update SESSION_PLAN.md ledger row 2 to ✅ with commit hashes.
- No external prep needed for S3.

---

### S3 — Eval framework + Generator

**Stage:** 1 | **P-NN:** P-12 + P-08 | **Tier:** All

**Pre-conditions:**
- S2 complete (P-08 Generator emits files via the backup-first wrapper that
  shares the audit-log pattern for File operations).
- `fixtures/aws-to-azure-real/` checked out (already done in S1).

**User input expected during:** *(none significant)*

**Autonomous work:**

1. **P-12 — Eval framework** (`libs/eval/`):
   - `GoldenMigration { source_tf, expected_target_tf, expected_audit_summary }`.
   - `EvalRunner::run(suite)` — executes the deterministic pipeline
     (Scanner → … → output) against each golden migration.
   - `EvalResult` includes pass/fail + diff + token-cost (token cost = 0
     in S3 since Mapper isn't ready; placeholder for S4).
   - 3 hand-curated minimal golden migrations seeded as fixtures.
   - CI integration stub (Article XII rule 4 gate; activates fully in S7).

2. **P-08 — Generator** (`libs/engine/src/generator/`):
   - Templates for AWS → Azure resource emission (5-10 resource types
     covering the PratikMahajan fixture's most common types).
   - `hcl-rs` serialization to produce target `.tf` files.
   - Backup-first wrapper: existing files → `.backup/{run_id}/` before write
     (Article V; pattern from `stakpak_arch.md` §28).
   - Round-trip property: emit → re-parse via Scanner == structural equivalent.

**Deliverables:**
- `specs/008-generator/` (6 artifacts)
- `specs/012-eval-framework/` (6 artifacts)
- `libs/engine/src/generator/{mod,templates,emitter,backup}.rs`
- `libs/eval/src/{golden,runner,scorer,errors}.rs`
- `terrashift-evals/` directory with 3 minimal golden migrations
- Tests: round-trip emit, backup-first preservation, eval against goldens

**Success criteria:**
- `cargo test --workspace` passes
- Eval framework runs against 3 goldens and reports pass/fail
- Generator emits HCL for 5 AWS-to-Azure resource pairs that re-parses cleanly

**Estimated duration:** 4-6 hours.

**Hand-off to S4:**
- **You sign up at https://console.groq.com (free, no credit card)** and put
  the API key somewhere accessible (we'll set `GROQ_API_KEY=...`).
- Optionally: read `pre-flight.md` Decision 9 to understand why Groq /
  Llama 3.3 70B was picked.

---

### S4 — LLM call (Groq) + Mapper + Validator

**Stage:** 1 | **P-NN:** P-03 + P-05 + P-06 | **Tier:** All

**Pre-conditions:**
- **`GROQ_API_KEY` env var set** (you grab from https://console.groq.com)
- S2 + S3 done

**User input expected during:**
- Confirm the 5-layer model resolution defaults (clarify will ask: "is Groq
  acceptable as both eco AND smart tier for Stage 1?" — recommended yes
  per pre-flight.md Decision 9).
- Confirm the Mapper prompt approach (single LLM call vs few-shot).

**Autonomous work:**

1. **P-03 — LLM call via stakai** (`libs/ai/`):
   - `Client` wraps `stakai::Inference`.
   - 5-layer model resolution: profile → CLI flag → /model → per-call override.
   - Tier-aware API: `complete(Tier::Eco | Smart, prompt)`.
   - Provider auto-registration (silent omission when no API key).
   - Policy/preference field split per Article V.
   - `~/.terrashift/config.toml` schema with Groq under
     `[providers.groq] type = "openai-compatible"`.
   - Integration test using real Groq API call to `llama-3.3-70b-versatile`.

2. **P-05 — Mapper** (`libs/engine/src/mapper/`):
   - Single LLM call producing strict JSON conforming to `schemars`-derived
     `MappingPlan` schema.
   - Cache by `EstateInventory` hash (per Article XII rule 2).
   - Article XIII rule 1 enforcement: every Mapper run goes through
     `ContextReducer` (will be a Stage 1 stub; full ContextReducer in S9).

3. **P-06 — Validator** (`libs/engine/src/validator/`):
   - Pure deterministic schema check between Mapper output and
     `libs/knowledge`'s schema cache.
   - Article III gate: hallucinated attributes → loud failure.
   - `ValidationReport { passed, warnings, errors }`.

**Deliverables:**
- `specs/003-llm-byok-resolution/`, `specs/005-mapper/`, `specs/006-validator/`
- `libs/ai/src/{client,resolver,routing,streaming}.rs`
- `libs/engine/src/mapper/{mod,prompt,schema,cache}.rs`
- `libs/engine/src/validator/{mod,checker,errors}.rs`
- 3 commits

**Success criteria:**
- Mapper produces a valid `MappingPlan` for a 3-resource synthetic input
- Validator catches a deliberately-hallucinated attribute (test fixture)
- `cargo test --workspace` (including the Groq integration test) passes
- First end-to-end run: `Scanner → Mapper → Validator → Generator` on a
  trivial fixture produces target `.tf` files

**Estimated duration:** 5-7 hours.

**Hand-off to S5:**
- Decide which cloud you want to test `terraform apply` against. Lowest-risk:
  a free-tier AWS account with a dedicated test project (and a budget alarm).
  Set up STS AssumeRole or AWS access key with IAM permissions scoped to a
  single test region.
- Install Docker (the Executor sandbox uses it).

---

### S5 — Sandboxed Executor + Credential broker

**Stage:** 1 | **P-NN:** P-09 + P-10 | **Tier:** All

**Pre-conditions:**
- S4 done.
- **AWS test account credentials available** (or GCP / Azure equivalent).
- Docker installed and running.

**User input expected during:**
- Confirm credential resolution method (STS AssumeRole vs static keys —
  recommend STS).
- Confirm the test cloud account's region + budget cap.
- Approve the first real `terraform apply` interactively.

**Autonomous work:**

1. **P-10 — Credential broker** (`libs/creds/`):
   - `CredentialBroker` async trait with `fetch_aws/gcp/azure(role)`.
   - Short-lived federated tokens (STS, ADC + WIF, managed identity).
   - `{{secret:name}}` substitution at tool-execution boundary.
   - Pre-prompt scrubber (gitleaks + entropy filter).
   - `Zeroizing<Credential>` wrapper, audit-logged on every operation.
   - Same broker handles LLM provider keys.

2. **P-09 — Sandboxed Executor** (`libs/engine/src/executor/`):
   - Docker-isolated `terraform plan` + `apply` per migration.
   - Tree-sitter command-level approval via `libs/shell-tool-approvals`
     (Article XIII rule 6 — never approve `terraform apply` wholesale).
   - Stream output via tracing spans.
   - Audit log on every action.

**Deliverables:**
- `specs/009-executor-sandbox/`, `specs/010-credential-broker/`
- `libs/creds/src/{broker,aws,gcp,azure,scrubber,zeroize_wrapper,hooks}.rs`
- `libs/engine/src/executor/{mod,sandbox,approval,output}.rs`
- `libs/shell-tool-approvals/src/{parser,policy,resolver}.rs` (filled in)
- Integration test: `terraform apply` of a single-resource VPC against the
  test account, then `terraform destroy`
- 2 commits

**Success criteria:**
- A real (1-resource) `terraform apply` runs end-to-end via the Executor
- Credentials never appear in any log/trace/audit entry (verified by grep)
- Audit log contains `CredentialResolution` + `ToolExecution` for the apply

**Estimated duration:** 6-8 hours (real cloud work — slower).

**Hand-off to S6:**
- Confirm the GitHub repo URL for Terrashift (we'll wire it for Release pipeline).
- Decide the binary distribution targets (Linux x86_64 + macOS arm64 minimum
  per pre-flight; add Windows if you want).

---

### S6 — TUI slash commands + Release pipeline

**Stage:** 1 | **P-NN:** P-14 + P-13 | **Tier:** All

**Pre-conditions:**
- S5 done.
- GitHub remote configured (`git remote add origin ...`).
- Optional: Homebrew tap repo (deferred to Stage 6 if not ready).

**User input expected during:**
- Confirm slash command set: `/help`, `/plan`, `/cost`, `/migrate`,
  `/rollback`, `/audit`, `/compact`, `/checkpoint` (recommend yes — matches
  v5 plan).

**Autonomous work:**

1. **P-14 — Slash commands** (`tui/src/commands/`):
   - Per-file pattern (Claude Code style — TERRASHIFT_MAPPING.md §F1).
   - Each command in its own file with metadata header + handler.
   - Registry walks the directory at startup.

2. **P-13 — Release pipeline** (`.github/workflows/release.yml`):
   - Tag-triggered (`v*`).
   - Matrix: Linux x86_64 (musl static), macOS arm64.
   - rustls-tls everywhere (no OpenSSL).
   - SHA256 sums + GitHub Release with cliff-generated changelog.
   - Adapt from Stakpak's `build-and-release.yml`.

**Deliverables:**
- `specs/013-release-pipeline/`, `specs/014-tui-slash-commands/`
- `tui/src/commands/{help,plan,cost,migrate,rollback,audit,compact,checkpoint}.rs`
- `.github/workflows/release.yml` + `cliff.toml`
- First tagged release: `v0.1.0` (manual trigger to verify pipeline works)
- 2 commits + 1 tag

**Success criteria:**
- `terrashift /help` renders the full command list
- A `v0.1.0` tag pushes a binary to GitHub Releases
- `cargo install --git https://github.com/<you>/terrashift` installs cleanly

**Estimated duration:** 5-6 hours.

**Hand-off to S7:**
- Pick 7 more golden migrations beyond the 3 from S3. Suggested: 3 from
  Pratik fixture's other modules (security_group, lambda, dynamodb), 4 hand-
  written for edge cases (multi-environment, count meta-arg, data sources).

---

## S7 → S37 — High-level summaries

(Each will get full detail when its turn comes; brief here so you can see
the shape.)

| # | Title | Key deliverable | User input needed |
|---|---|---|---|
| 7 | Eval expansion (10 golden migrations) | `terrashift-evals/` repo with 10 paired examples | Pick golden migration sources |
| 8 | Stage 1 exit gate (P-16) | Gate review report; remediation tickets if needed | Approve gate or pick deferral |
| 9 | Agent loop kernel | `run_agent`, `ApprovalStateMachine`, retry, stream — completes `libs/agent-core` | None |
| 10 | Recovery agent | `libs/engine/src/recovery/` with bounded ReAct loop | Confirm retry budget cap |
| 11 | Cost Optimizer + Infracost | `libs/engine/src/cost_optimizer/` + Cost Service crate | Sign up at infracost.io |
| 12 | WIF/OIDC modernization | Replace static keys with federated tokens for AWS+GCP+Azure | Cloud admin to set up trust policies |
| 13 | Detached mode + notifications | `terrashift migrate --detach` + Slack/email/webhook | Pick notification channels |
| 14 | Stage 2 hardening + eval | Token cost regression CI gate enforced | None |
| 15 | Stage 2 exit gate | Gate review | Approve |
| 16 | Azure provider (source + target) | Azure-specific Mapper extensions, schema cache | Azure test account |
| 17 | RAG production (LanceDB) | `libs/knowledge/src/vector_store.rs` + cross-encoder reranker | None |
| 18 | LLM tier router + fallback | Multi-provider routing (Anthropic, OpenAI, Gemini, Groq) | Pick which providers to support |
| 19 | GitHub VCS integration | Clone, branch, PR creation, webhook listener | GitHub PAT or App |
| 20 | All 6 cloud-pair smoke tests | GCP↔AWS↔Azure all directions tested | Test accounts on all 3 clouds |
| 21 | Stage 3 exit gate | Gate review | Approve |
| 22 | Data migration service | DMS / Storage Transfer / AzCopy wrappers | Hire/dedicate data eng helper for design review |
| 23 | Strategy Selector agent | Picks per-resource: copy / dual-write / replicate-then-promote | None |
| 24 | Cutover agent + DNS plug-ins | Per-resource cutover plan, Route53/Cloud DNS/Azure DNS/Cloudflare | DNS provider credentials |
| 25 | Secrets migration + parity verify | Secret Manager values copied; HCL refs rewritten | None |
| 26 | Stage 4 exit gate | Gate review | Approve |
| 27 | HCL constructs (dynamic, count, for_each) | Re-enable Stage 1 deferrals; full Terraform completeness | None |
| 28 | Module ecosystem + Terragrunt + OpenTofu | Detect community modules; preserve Terragrunt wrappers | None |
| 29 | Policy translation + plugin system | GCP Org Policies → AWS SCPs → Azure Policy; first 3 plugins (Datadog, GitHub, Kubernetes) | None |
| 30 | Real customer migration validation | Migrate a real customer's 100+ resource repo | Customer commitment |
| 31 | Stage 5 exit gate | Gate review | Approve |
| 32 | Quality + reliability + chaos testing | LLM-down, cloud-rate-limited, network-partition tests | None |
| 33 | Sandbox/staging + drift detection | Production-grade sandbox; periodic drift scan | None |
| 34 | SaaS multi-tenancy + bridge runner | Row-level security, per-tenant Vault paths, agentless mode | Decide self-host vs managed |
| 35 | GitLab + Bitbucket integrations | Mirror GitHub integration shape | GitLab/Bitbucket PATs |
| 36 | Compliance (SOC2, signed audit export, GDPR) | SOC2 Type 1 audit-ready posture | Engage compliance auditor |
| 37 | GA gate + reference customer in production | Reference customer running on SaaS for 14 days | Customer commitment |

---

## Stage exit criteria (where you decide "tier reached")

Cite `terrashift_plan.md` §17 + Article VIII for the canonical list.

### Stage 1 — MVP gate (after S8)
Per `terrashift_prompts.md` P-16:
1. Demo passes for 15-resource sample end-to-end
2. Audit log signed (chain integrity verified)
3. Re-running migrate produces identical output (Article VI)
4. Token cost <$15 for sample migration (Article XII)
5. All 13 constitution articles cited in at least one PR
6. Single-binary distribution works on a fresh Linux + macOS install

### Stage 2 — Agentic gate (after S15)
1. 5 different small-but-real GCP→AWS migrations succeed E2E
2. 3 deliberately-broken sample migrations get fixed by Recovery agent within 5 retries
3. Cost reports validated against manual Infracost runs (within 5%)
4. Cost Optimizer refines plan on at least 2 scenarios (gp2→gp3, x86→Graviton)
5. WIF/OIDC verified end-to-end; no long-lived keys in test runs
6. Token cost regression: Stage 2 happy-path costs ≤1.3× Stage 1 happy-path

### Stage 3 — Multi-cloud gate (after S21)
1. All 6 directional pairs pass smoke tests
2. LLM fallback verified (kill primary mid-migration, fallback completes)
3. Cost + latency comparison report across all 3 providers
4. GitHub PR auto-created in target repo with generated HCL
5. RAG hit rate >90% on the eval corpus

### Stage 4 — Data + cutover gate (after S26)
1. 3 stateful E2E migrations including data
2. Zero data loss verified by row-count + checksum
3. Cutover with <30s downtime on a representative app
4. Rollback verified (partial cutover reversible without data loss)

### Stage 5 — Structural fidelity gate (after S31)
1. Migrate a real customer's production-grade repo (100+ resources, 5+ modules)
2. Diff vs human-authored target: <10% structural drift
3. 2 community plugins authored by external contributors
4. Terragrunt repo migrates with wrapper preserved
5. OpenTofu source migrates to OpenTofu target

### Stage 6 — GA gate (after S37)
1. Pen test passed (no critical/high)
2. Reference customer on SaaS for ≥14 days
3. 99.5% migration success rate on eval corpus over 30 days
4. SOC2 Type 1 audit-ready
5. Public docs site live; `cargo install` one-liner works

---

## How to start the next session

1. **Open Claude Code** in `C:\Users\goda\Desktop\terrashift\`.
2. **Check this file's "Session ledger" table** to find the next session
   marked ⏳ Next or Planned.
3. **Verify pre-conditions** in that session's detail block.
4. **Tell the agent:** "Start session NN per SESSION_PLAN.md."
5. **The agent will:**
   - Re-read SESSION_PLAN.md to get the session's spec
   - Re-read TERRASHIFT_MAPPING.md to get the seam context
   - Re-read pre-flight.md decisions for environment
   - Run the Spec Kit cycle (specify → clarify → plan → tasks → implement →
     analyze → checklist) for each P-NN in the session
   - Commit each P-NN with `feat(pNN): ...` per Article VII
   - **Update this file's ledger** with the commit hash + status before stopping

---

## Anti-patterns when running sessions

1. **Don't compress two sessions into one** even if you have time — the
   commit boundaries are the natural review checkpoints.
2. **Don't skip the clarify phase** even when the answer feels obvious — it's
   where ambiguities surface (per `superpowers:test-driven-development`).
3. **Don't run cloud-touching sessions (S5, S12, S20, etc.) without a
   budget cap** on the test account.
4. **Don't merge LLM-using sessions (S4, S10, S11) without checking the
   token-cost regression** vs the eval baseline (Article XII rule 4).
5. **Don't approve `terraform apply` wholesale** in S5 — use the tree-sitter
   command-level approval from `libs/shell-tool-approvals` (Article XIII rule 6).

---

## Maintenance

This file is the source of truth for "where are we." Keep it updated:

- After each session's final commit, edit the ledger row's Status to ✅ and
  add the commit hash. Add any "questions for next session" to that
  session's detail block.
- If a session is split (too big to fit), insert a "S{NN}a" / "S{NN}b" pair
  and renumber subsequent sessions.
- If a session is merged with the next (pre-conditions all met, tighter
  scope), document the merge in both rows.
- Article XI applies — substantial changes to the session count or sequence
  should be a one-paragraph entry in this file's git log.
