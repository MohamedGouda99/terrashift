# Terrashift

Cross-cloud Terraform migration, built as a single Rust binary.

Migrates Terraform-managed infrastructure between cloud providers (GCP ⇄ AWS ⇄ Azure)
using LLMs only where they earn their place, deterministic code everywhere else.

## Status

**Stage 1 — MVP** (in progress). See `terrashift_plan.md` §17 for the stage roadmap.

## Documentation

- [`terrashift_plan.md`](terrashift_plan.md) — architecture and stages
- [`CONSTITUTION.md`](CONSTITUTION.md) — 13 articles governing how we build
- [`terrashift_prompts.md`](terrashift_prompts.md) — implementation prompts (P-00..P-22)
- [`TERRASHIFT_MAPPING.md`](TERRASHIFT_MAPPING.md) — Stakpak seam → Terrashift mapping (output of P-00)
- [`pre-flight.md`](pre-flight.md) — environment setup decisions for this machine

## Reference codebases

Patterns are mirrored from two open-source codebases checked out locally:

- **Stakpak** (primary architectural reference, Apache 2.0) — see `refs/stakpak/`
- **Claude Code source** (secondary, agent-loop concepts) — see `refs/claude-code/`

The canonical architectural reference is `refs/stakpak_arch.md` (~2,840 lines).

See [`ATTRIBUTIONS.md`](ATTRIBUTIONS.md) for license attribution.
