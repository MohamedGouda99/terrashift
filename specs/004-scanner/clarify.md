# Clarifications — P-04

User authorized "answer Spec Kit questions on my behalf." Decisions follow
constitution + TERRASHIFT_MAPPING.md decision tree.

## Q1: Where do EstateInventory types live — `libs/shared` or `libs/engine`?

**Options:** (a) `libs/shared/src/inventory.rs` — shared with other crates;
(b) `libs/engine/src/scanner/inventory.rs` — engine-internal.

**Decision:** **`libs/engine/src/scanner/inventory.rs` (b)** for Stage 1.
Mapper (P-05) will consume EstateInventory and lives in `libs/engine`. Audit
log doesn't need it (it serializes its own AuditPayload variants). When/if
Stage 6 SaaS API needs to expose inventory shape externally, move to
`libs/shared` then. Lower coupling now; refactor when actually needed.

## Q2: Attribute values — raw string vs typed structure?

**Options:** (a) `String` (raw HCL expression) — simple, Mapper parses;
(b) `serde_json::Value` — structured, but loses HCL refs;
(c) `hcl::Expression` — full fidelity, but couples Mapper to hcl-rs.

**Decision:** **String (a)** for Stage 1. Mapper needs the raw expression
text for two reasons: (1) `var.X` refs must be preserved for the Mapper to
resolve against the variables list; (2) The Mapper's prompt to the LLM works
better with raw HCL than with serialized JSON. Trade-off: Mapper needs a
mini-parser for value lookup, but that's tiny vs the Mapper's main job.
Document the design as a deliberate Stage 1 choice; revisit if Mapper hits
parsing pain.

## Q3: Source spans — full file:line:col, or just file path?

**Options:** (a) Full `(file, line, col, end_line, end_col)` — best for
error UX; (b) Just `(file, line)` — simpler.

**Decision:** **`(file, line, col)` start position only** — minimal span
that's still useful for Article IV (loud failures). hcl-rs's `Span` type
gives byte offsets; we convert to (line, col) at parse time. End position is
nice-to-have but defer to Stage 5 when we ship structured error UI.

## Q4: How aggressively to detect "Stage 1 unsupported"?

**Options:** (a) Hard-fail on any unsupported feature (dynamic, count,
for_each, provisioners, complex modules); (b) Warn-but-continue;
(c) Hard-fail on dynamic blocks only (the §11 most-broken case);
(d) Hard-fail on a documented allowlist.

**Decision:** **Hard-fail on dynamic blocks only (c)** for Stage 1 P-04.
Other features (count, for_each, provisioners) are common enough that
hard-failing would block the Pratik fixture. Capture them in `notes` and
continue; let the Mapper decide if it can handle them. Dynamic blocks are
genuinely broken — they require runtime expansion which we don't do in
Stage 1.

Document the choice in `ScannedFile::scan_notes` — every "could be a
problem later" gets a note that flows up to the user.

## Q5: Data sources — full parse or skip?

**Options:** (a) Parse them; (b) Skip them in Stage 1 (focus on resources).

**Decision:** **Parse them (a)**. Data sources are common (`data
"aws_caller_identity" "current"`) and the Mapper needs to know about them
for ID lookups. Cost is trivial — same parse path as resource blocks, just a
different block kind. Skipping creates a "what about data?" question at
Mapper time.

## Q6: Test against the real fixture (~80 .tf files) or only synthetic samples?

**Options:** (a) Both — synthetic for unit tests, fixture for one big
integration test; (b) Only synthetic; (c) Only fixture.

**Decision:** **Both (a)**. Synthetic samples for the 5 spec'd test cases
(deterministic, no GPL dependency). One additional integration test that
walks `fixtures/aws-to-azure-real/aws/modules/vpc/` (small subset, well-
defined) to confirm the parser handles real-world Terraform 0.12 syntax.
Skip the integration test if the fixture isn't checked out (graceful, not
hard-fail).

---

All clarifications resolved. Proceeding to plan + tasks + implement.
