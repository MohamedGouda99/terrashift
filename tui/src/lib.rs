//! Terrashift TUI — Ratatui-based terminal UI.
//!
//! Pattern: stakpak_arch.md §16 (TUI structure: AppState, services/handlers,
//! dual-channel mpsc contract InputEvent / OutputEvent).
//! Constitution: Article XIII rule 8 (don't add new InputEvent/OutputEvent
//! variants without updating is_backend_event() — silent UI freezes are
//! the failure mode).
//!
//! ## Modules
//! - `commands` — slash-command registry + per-file commands (P-14;
//!   adopts Claude Code's per-file pattern per TERRASHIFT_MAPPING.md §F1).
//!   The Ratatui mainloop that *consumes* `CommandOutcome` arrives in
//!   Stage 2 / P-14b.

pub mod commands;
