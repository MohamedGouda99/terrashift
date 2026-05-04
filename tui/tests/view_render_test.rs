#![allow(clippy::expect_used, clippy::unwrap_used)]
//! View rendering tests — drive ratatui's `TestBackend` and assert the
//! frame doesn't panic AND contains the bits we expect.
//!
//! `TestBackend` writes to an in-memory buffer instead of the real terminal,
//! so no raw mode or alt-screen needed. This catches geometry bugs (cursor
//! placement, scroll math) and missing-content regressions.

use ratatui::{backend::TestBackend, Terminal};
use terrashift_tui::{view, AppState, MessageKind, StatusInfo};

fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 30);
    Terminal::new(backend).expect("create test terminal")
}

fn buffer_contains(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = buf.cell((x, y)).expect("cell");
            all.push_str(cell.symbol());
        }
        all.push('\n');
    }
    all.contains(needle)
}

#[test]
fn view_renders_empty_state_without_panic() {
    let mut terminal = make_terminal();
    let app = AppState::new(StatusInfo::default());
    terminal.draw(|f| view::view(f, &app)).expect("draw");
    assert!(buffer_contains(&terminal, "Terrashift"));
}

#[test]
fn view_renders_status_footer_with_profile_and_tier() {
    let mut terminal = make_terminal();
    let mut app = AppState::new(StatusInfo {
        profile_path: Some(std::path::PathBuf::from("C:/test/profile.toml")),
        seed_resources: Some(173),
        tier: "smart".to_string(),
    });
    app.push(MessageKind::Banner, "ready");
    terminal.draw(|f| view::view(f, &app)).expect("draw");
    assert!(buffer_contains(&terminal, "smart"));
    assert!(buffer_contains(&terminal, "173"));
}

#[test]
fn view_renders_long_message_history_without_panic() {
    let mut terminal = make_terminal();
    let mut app = AppState::new(StatusInfo::default());
    for i in 0..200 {
        app.push(MessageKind::Output, format!("line {i}"));
    }
    terminal.draw(|f| view::view(f, &app)).expect("draw");
    // Only the last lines fit; just assert a recent one shows.
    assert!(buffer_contains(&terminal, "line 199"));
}

#[test]
fn view_renders_with_long_input_without_panic() {
    let mut terminal = make_terminal();
    let mut app = AppState::new(StatusInfo::default());
    let long: String = "x".repeat(500);
    for c in long.chars() {
        app.input_insert(c);
    }
    terminal.draw(|f| view::view(f, &app)).expect("draw");
    // Cursor logic must not panic on long input that overflows the input box.
}

#[test]
fn view_renders_unicode_in_messages_and_input() {
    let mut terminal = make_terminal();
    let mut app = AppState::new(StatusInfo::default());
    app.push(MessageKind::Output, "café 🚀 résumé");
    for c in "🌍 hello".chars() {
        app.input_insert(c);
    }
    terminal.draw(|f| view::view(f, &app)).expect("draw");
    assert!(buffer_contains(&terminal, "café"));
}

#[test]
fn view_renders_tiny_terminal_without_panic() {
    // Pathologically small: 20x5. Layout should saturate, not panic.
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).expect("term");
    let mut app = AppState::new(StatusInfo::default());
    app.push(MessageKind::Output, "hi");
    terminal.draw(|f| view::view(f, &app)).expect("draw");
}

#[test]
fn view_handles_scroll_offset_larger_than_history() {
    let mut terminal = make_terminal();
    let mut app = AppState::new(StatusInfo::default());
    app.push(MessageKind::Output, "only one");
    app.scroll_offset = 999; // operator scrolled way past anything sensible
    terminal.draw(|f| view::view(f, &app)).expect("draw");
    // Should still render the one message we have without panicking.
    assert!(buffer_contains(&terminal, "only one"));
}
