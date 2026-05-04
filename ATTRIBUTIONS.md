# Attributions

## Terrashift's own license

Copyright © 2026 Mohamed Gouda. All Rights Reserved.

The Terrashift project — the original code, documentation, schemas, and
assets in this repository — is licensed under the **Terrashift Source-
Available License v1.0** (see [`LICENSE`](LICENSE)). This is a
proprietary, source-available license. Personal non-commercial use is
permitted; commercial use requires a separate written agreement.

This file documents the third-party works that informed Terrashift's
design and the dependencies it consumes, in compliance with their
respective licenses and Article II of the constitution.

## Third-party patterns Terrashift adapts

### Stakpak — primary architectural reference

- **Source:** https://github.com/stakpak/agent
- **License:** Apache License 2.0
- **What we did:** *adapted conceptual patterns* — workspace layout, Tool
  trait pattern, MCP layer (rmcp), secret substitution, privacy-mode
  redaction, subagent permission model, reversible file operations,
  rulebook format, checkpointing, observability via `tracing`, bulk
  message approval, real-time progress streaming, asynchronous task
  management. See `docs/architecture/terrashift_plan.md` §16.1 for the
  full pattern table.
- **License interaction:** Apache 2.0 permits derivative works under any
  license. Our adapted code is original Terrashift implementation and
  falls under Terrashift's proprietary license. Where any verbatim
  Stakpak source is used (e.g., trait signatures we mirror), it remains
  under Apache 2.0 with attribution preserved.
- **stakai LLM SDK:** Direct dependency at `libs/ai`. Linked as a
  workspace crate; remains under its original Apache 2.0 license.

### Claude Code source — secondary reference for agent-loop concepts

- **Source:** https://github.com/MohamedGouda99/claude-cli-src
- **What we did:** *translated concepts, did not fork* — conceptual `Tool`
  shape (`Tool.ts`), conversation state machine (`QueryEngine.ts`,
  `query.ts`), per-file slash-command pattern (`commands/`), three-mode
  compaction (`services/compact/`), skills system (`skills/`), async
  generator streaming (`query.ts`), background task taxonomy (`tasks/`),
  plugin definition shape, cost-tracking hooks (`costHook.ts`),
  permission dialog launcher (`dialogLaunchers.tsx`).
- **License interaction:** No verbatim Claude Code source ships in
  Terrashift. Concepts are reimplemented in Rust idioms. The Rust
  implementations are original Terrashift code under the proprietary
  license. The Claude Code TypeScript source remains property of its
  authors and is consulted only as a reference (per-developer junction
  in `refs/claude-code/`, not redistributed).
- See `docs/architecture/terrashift_plan.md` §16.2 for the full pattern table.

## Workspace dependencies

Terrashift depends on numerous open-source crates. Each retains its own
license; consuming a permissively-licensed crate as a runtime dependency
does not require relicensing Terrashift's own code. Run `cargo license`
for the full per-crate license report. All workspace-level dependencies
are permissively licensed (MIT, Apache-2.0, MPL-2.0, BSD).

Notable direct dependencies:

| Crate | License |
|---|---|
| `tokio` | MIT |
| `serde`, `serde_json` | MIT OR Apache-2.0 |
| `ratatui`, `crossterm` | MIT |
| `clap` | MIT OR Apache-2.0 |
| `reqwest` | MIT OR Apache-2.0 |
| `stakai` | Apache-2.0 |
| `rmcp` | MIT |
| `hcl-rs` | MIT |
| `lancedb` | Apache-2.0 |
| `sqlx` | MIT OR Apache-2.0 |
| `zeroize` | MIT OR Apache-2.0 |
| `ed25519-dalek` | BSD-3-Clause |

Full report: `cargo license --json > ATTRIBUTIONS-deps.json` (run before each release).
