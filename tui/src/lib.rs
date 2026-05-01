//! Terrashift TUI — Ratatui-based terminal UI.
//!
//! Pattern: stakpak_arch.md section 16 (TUI structure: AppState, services/handlers,
//! dual-channel mpsc contract InputEvent / OutputEvent).
//! Constitution: Article XIII rule 8 (don't add new InputEvent/OutputEvent
//! variants without updating is_backend_event() — silent UI freezes are the
//! failure mode).
//!
//! Implementation arrives in P-14 (slash commands).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        // Sentinel test — ensures the crate builds standalone.
    }
}
