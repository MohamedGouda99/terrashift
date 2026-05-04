//! `AppState` behavior tests — input editing, scrolling, message buffering.
//!
//! Why these matter for the MVP: these are the data-model invariants the
//! TUI driver depends on. A bug in `input_backspace` panicking on a
//! multibyte character would crash the TUI mid-keystroke. We caught one
//! `clippy::string_slice` flag previously by switching to `.get(..)`, which
//! changed behavior in subtle ways — these tests pin the new behavior down.

use terrashift_tui::{AppState, MessageKind, StatusInfo};

fn fresh() -> AppState {
    AppState::new(StatusInfo::default())
}

// ─────────────────────────────────────────────────────────────────────────
// Input editing — happy path
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn input_insert_appends_at_cursor() {
    let mut s = fresh();
    for c in "hello".chars() {
        s.input_insert(c);
    }
    assert_eq!(s.input_buffer, "hello");
    assert_eq!(s.cursor, 5);
}

#[test]
fn backspace_removes_previous_char() {
    let mut s = fresh();
    for c in "hello".chars() {
        s.input_insert(c);
    }
    s.input_backspace();
    assert_eq!(s.input_buffer, "hell");
    assert_eq!(s.cursor, 4);
}

#[test]
fn backspace_at_start_is_noop() {
    let mut s = fresh();
    s.cursor = 0;
    s.input_backspace();
    assert_eq!(s.input_buffer, "");
    assert_eq!(s.cursor, 0);
}

#[test]
fn left_right_move_cursor_within_bounds() {
    let mut s = fresh();
    for c in "abc".chars() {
        s.input_insert(c);
    }
    assert_eq!(s.cursor, 3);
    s.input_left();
    assert_eq!(s.cursor, 2);
    s.input_left();
    s.input_left();
    s.input_left(); // past 0 → no underflow
    assert_eq!(s.cursor, 0);
    s.input_right();
    s.input_right();
    s.input_right();
    s.input_right(); // past len → no overflow
    assert_eq!(s.cursor, 3);
}

// ─────────────────────────────────────────────────────────────────────────
// UTF-8 safety — multibyte characters must not split a char boundary
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn input_handles_multibyte_chars_without_panic() {
    let mut s = fresh();
    for c in "café".chars() {
        s.input_insert(c);
    }
    // 'é' is 2 bytes in UTF-8 → buffer is 5 bytes long, cursor at 5.
    assert_eq!(s.input_buffer, "café");
    assert_eq!(s.cursor, 5);

    s.input_backspace(); // remove 'é' (2 bytes)
    assert_eq!(s.input_buffer, "caf");
    assert_eq!(s.cursor, 3);
}

#[test]
fn input_handles_emoji_without_panic() {
    let mut s = fresh();
    for c in "hi🚀".chars() {
        s.input_insert(c);
    }
    // 🚀 is 4 bytes in UTF-8.
    assert_eq!(s.input_buffer, "hi🚀");
    assert_eq!(s.cursor, 6);

    s.input_left(); // moves over the emoji as one unit
    assert_eq!(s.cursor, 2);
    s.input_right();
    assert_eq!(s.cursor, 6);

    s.input_backspace(); // remove 🚀
    assert_eq!(s.input_buffer, "hi");
}

#[test]
fn delete_at_end_is_noop() {
    let mut s = fresh();
    for c in "abc".chars() {
        s.input_insert(c);
    }
    // Cursor sits at end (3) — delete has nothing to consume.
    s.input_delete();
    assert_eq!(s.input_buffer, "abc");
    assert_eq!(s.cursor, 3);
}

#[test]
fn delete_in_middle_consumes_next_char() {
    let mut s = fresh();
    for c in "abcd".chars() {
        s.input_insert(c);
    }
    s.cursor = 1; // between 'a' and 'b'
    s.input_delete();
    assert_eq!(s.input_buffer, "acd");
    assert_eq!(s.cursor, 1);
}

// ─────────────────────────────────────────────────────────────────────────
// take_input clears state
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn take_input_returns_buffer_and_resets() {
    let mut s = fresh();
    for c in "/help".chars() {
        s.input_insert(c);
    }
    let taken = s.take_input();
    assert_eq!(taken, "/help");
    assert_eq!(s.input_buffer, "");
    assert_eq!(s.cursor, 0);
}

// ─────────────────────────────────────────────────────────────────────────
// Scrolling — bounds must hold under saturating arithmetic
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn scroll_down_at_zero_does_not_underflow() {
    let mut s = fresh();
    s.scroll_offset = 0;
    s.scroll_down(50); // saturating sub
    assert_eq!(s.scroll_offset, 0);
}

#[test]
fn scroll_up_does_not_overflow_usize() {
    let mut s = fresh();
    s.scroll_offset = usize::MAX - 2;
    s.scroll_up(10); // saturating add
    assert_eq!(s.scroll_offset, usize::MAX);
}

#[test]
fn pushing_message_pins_to_bottom() {
    let mut s = fresh();
    s.scroll_offset = 25;
    s.push(MessageKind::Output, "new message");
    assert_eq!(
        s.scroll_offset, 0,
        "new message should pin viewport to bottom (operator hasn't scrolled since)"
    );
}

#[test]
fn push_user_input_prefixes_caret() {
    let mut s = fresh();
    s.push_user_input("/help");
    assert_eq!(s.messages.len(), 1);
    assert_eq!(s.messages[0].kind, MessageKind::UserInput);
    assert!(s.messages[0].text.contains("/help"));
    assert!(s.messages[0].text.contains("▶"));
}

#[test]
fn request_exit_sets_flag() {
    let mut s = fresh();
    assert!(!s.should_exit);
    s.request_exit();
    assert!(s.should_exit);
}
