# Attributions

Terrashift is built on patterns and code from two open-source projects.
Both attributions are required by Article II of the constitution.

## Stakpak — primary architectural reference

- **Source:** https://github.com/stakpak/agent
- **License:** Apache License 2.0
- **Adopted:** workspace layout, Tool trait pattern, MCP layer (rmcp), secret
  substitution, privacy-mode redaction, subagent permission model, reversible
  file operations, rulebook format, checkpointing, observability via `tracing`,
  bulk message approval, real-time progress streaming, asynchronous task
  management. See `terrashift_plan.md` §16.1 for the full pattern table.
- **stakai LLM SDK:** Direct dependency at `libs/ai`. Apache 2.0.

## Claude Code source — secondary reference for agent-loop concepts

- **Source:** https://github.com/MohamedGouda99/claude-cli-src
- **Adopted:** conceptual `Tool` shape (`Tool.ts`), conversation state machine
  (`QueryEngine.ts`, `query.ts`), per-file slash-command pattern (`commands/`),
  three-mode compaction (`services/compact/`), skills system (`skills/`),
  async generator streaming (`query.ts`), background task taxonomy (`tasks/`),
  plugin definition shape, cost-tracking hooks (`costHook.ts`),
  permission dialog launcher (`dialogLaunchers.tsx`).
- We translate concepts to Rust idioms; we do not fork the TypeScript source.
- See `terrashift_plan.md` §16.2 for the full pattern table.

## Workspace dependencies

This project depends on numerous open-source crates. Run `cargo license` to
generate the full license report. All workspace-level dependencies are
permissively licensed (MIT, Apache-2.0, MPL-2.0, BSD).

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
