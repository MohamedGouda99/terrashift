# Clarifications — P-02

User authorized "answer Spec Kit questions on my behalf using constitution + plan
as the decision tree." Decisions below applied that rule. Constitution Article II
(reference codebase discipline) drives most answers.

## Q1: Mirror Stakpak verbatim or adapt for Terrashift's domain?

**Options:** (a) Verbatim — same trait signatures, same imports; (b) adapt to
Terrashift naming (e.g., `MigrationToolExecutor`).

**Decision:** **Verbatim (a)**. User explicitly stated "Stakpak is primary."
Article II requires citing the source pattern. Renaming would obscure the
mirror. Tools _using_ ToolExecutor can have Terrashift-specific names
(ScannerExecutor, MapperExecutor) — that's a separate concern.

## Q2: Include a separate `Tool` trait (Claude Code style) or just `ToolExecutor`?

**Options:** (a) Just `ToolExecutor` (Stakpak shape — one trait, dispatches
by name); (b) Add `Tool` trait (per-tool struct that knows its own name +
description + schema + execute).

**Decision:** **Just `ToolExecutor` (a)**. Stakpak doesn't have a separate
`Tool` trait — `stakai::Tool` is the LLM-facing description, `ToolExecutor`
is the runtime dispatcher. TERRASHIFT_MAPPING.md §A row 9 tools the same
shape (`#[tool_router]` impl block, not per-tool trait). Diverging here
would create a maintenance burden vs Stakpak's shape.

The Claude Code "per-file" pattern in TERRASHIFT_MAPPING.md §F1 applies to
**slash commands** (P-14), not to the core ToolExecutor. Don't conflate.

## Q3: Add `ToolRegistry` (a HashMap-backed convenience) or skip it?

**Options:** (a) Add it — common need across P-04 to P-09; (b) skip — defer
to first crate that needs multi-tool dispatch.

**Decision:** **Add it (a)**. Without ToolRegistry, every crate that wants to
register multiple tools (`libs/engine` for Scanner+Mapper+Validator+...)
would re-invent the dispatch HashMap. ToolRegistry IS just a helper that
implements `ToolExecutor` by looking up `tool_call.name` — it's not a new
abstraction; it's an ergonomic wrapper around the existing trait.

Cite: `// Terrashift addition — not in Stakpak. Convenience layer over
ToolExecutor for multi-tool registration.`

## Q4: `AgentError` minimal vs full?

**Options:** (a) Full — all 9 Stakpak variants verbatim, with stub modules
for approval/checkpoint/stream; (b) minimal — only variants needed by P-02
(`ToolExecution`, `Hook`, `Cancelled`).

**Decision:** **Minimal (b)**. Stakpak's `AgentError` references modules
that don't exist in P-02 scope (approval, checkpoint, stream). Adding stub
modules just to satisfy the From impls is bloat. Each variant arrives with
its module: `Approval` in P-09, `Checkpoint` in P-14, `StreamAssembly` in
Stage 2.

The minimal `AgentError` will have `ToolExecution(String)`, `Hook(String)`,
`Inference(String)`, `Cancelled` — same signatures as Stakpak so future
expansion is purely additive.

## Q5: Add stakai dependency to agent-core?

**Options:** (a) Yes — needed for `Message`, `Model` in hooks; (b) no — define
a Terrashift-local `Message`/`Model` type.

**Decision:** **Yes (a)**. Stakpak's hooks.rs imports `stakai::{Message, Model}`
directly. Per Article II we mirror this. Defining a parallel Terrashift type
would diverge without benefit. stakai is already a workspace dependency
(`stakai = "0.3"` in root Cargo.toml).

---

All clarifications resolved. Proceeding to plan + tasks + implement.
