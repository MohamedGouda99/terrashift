# Clarifications — P-11

User-on-behalf decisions per constitution + plan.

## Q1: Signing key — per-process or per-migration (per run_id)?

**Decision:** **Per-migration (per run_id)**. Generated at run start, public
key persisted in run metadata (a `runs` table). Compromise of one run's key
doesn't invalidate other runs. Matches `terrashift_plan.md` §6.X.

## Q2: Hash algorithm — SHA-256 or BLAKE3?

**Decision:** **SHA-256**. Slower than BLAKE3 but standard, ubiquitously
supported in compliance tooling, and audit volume (~hundreds of entries per
migration) doesn't pressure performance. Article IX (data governance —
auditors expect SHA-256).

## Q3: Where does `AuditEntry::content_hash` exclude itself?

**Decision:** **Compute over canonical JSON of all fields EXCEPT
content_hash + signature** (those are derived). prev_hash IS included so
chain breaks are detectable. Use `serde_canonical_json` if available, else
hand-canonicalize via sorted-key JSON.

## Q4: Pre-write redaction — panic or return Err?

**Decision:** **Panic**. Per terrashift_plan.md §6.X "the writer is the LAST
line of defence — panics loudly". Returning Err invites callers to swallow.
Panic is loud + non-bypassable. The redaction check itself MUST be infallible
in design (the regex set is compiled once at startup; a regex compile failure
during construction returns Err and the writer never opens).

## Q5: Stage 1 redaction patterns — exhaustive or minimal?

**Decision:** **Minimal but sufficient** — enough to catch the most common
real leaks: AWS access keys (`AKIA[0-9A-Z]{16}`), GCP API keys (`AIza...`),
GitHub tokens (`ghp_...`), private key headers (`-----BEGIN.*PRIVATE KEY-----`),
high-entropy strings ≥40 chars. Full gitleaks rule set (~60 patterns)
arrives in S30 (security-auditor sub-agent uses it).

## Q6: SQLite vs file-based — which for audit?

**Decision:** **SQLite append-only**. Query patterns (filter by run_id, time
range, payload variant) want SQL. File-based would require either a separate
index or full-scans. Same sqlx pattern as P-07.

## Q7: `AuditWriterHook` location — `libs/audit` or `libs/engine`?

**Decision:** **`libs/audit/src/hooks.rs`**. The hook IS what makes the
audit log automatically populated; it belongs with the audit code. Engine
just registers it on agent construction (in S9 when agent loop arrives).
