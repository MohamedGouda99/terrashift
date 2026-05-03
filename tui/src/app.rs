//! `AppState` — interactive TUI state.
//!
//! Holds the message history, the input buffer, the scroll offset for
//! the message viewport, and the static status footer fields. Pure data
//! — no I/O, no rendering.

use std::path::PathBuf;

/// One item in the scrolling message area.
#[derive(Debug, Clone)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Welcome / informational text from the app itself.
    Banner,
    /// What the operator typed (echoed back into the history).
    UserInput,
    /// Successful command output (`CommandOutcome::Text`).
    Output,
    /// Error from a command (`CommandOutcome::Error`).
    Error,
    /// An `Action::*` outcome that the TUI doesn't yet wire to a runtime
    /// — rendered as a hint pointing operators at the shell subcommand.
    Hint,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub messages: Vec<Message>,
    pub input_buffer: String,
    pub cursor: usize,
    /// How many lines from the bottom the viewport is offset. `0` = pinned
    /// to bottom (latest message visible). Increments on PageUp / scroll-up.
    pub scroll_offset: usize,
    pub status: StatusInfo,
    pub should_exit: bool,
}

#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    pub profile_path: Option<PathBuf>,
    pub seed_resources: Option<usize>,
    pub tier: String,
}

impl AppState {
    pub fn new(status: StatusInfo) -> Self {
        Self {
            messages: Vec::new(),
            input_buffer: String::new(),
            cursor: 0,
            scroll_offset: 0,
            status,
            should_exit: false,
        }
    }

    pub fn push(&mut self, kind: MessageKind, text: impl Into<String>) {
        self.messages.push(Message {
            kind,
            text: text.into(),
        });
        // Pin to bottom whenever new content arrives — the natural shell
        // behaviour. Operators scroll back up explicitly with PageUp.
        self.scroll_offset = 0;
    }

    pub fn push_user_input(&mut self, raw: &str) {
        self.push(MessageKind::UserInput, format!("▶ {}", raw));
    }

    pub fn input_insert(&mut self, c: char) {
        self.input_buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Find prev char boundary.
        let prev = self
            .input_buffer
            .get(..self.cursor)
            .unwrap_or("")
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.input_buffer.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn input_delete(&mut self) {
        if self.cursor >= self.input_buffer.len() {
            return;
        }
        let next = self
            .input_buffer
            .get(self.cursor..)
            .unwrap_or("")
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.input_buffer.len());
        self.input_buffer.replace_range(self.cursor..next, "");
    }

    pub fn input_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self
            .input_buffer
            .get(..self.cursor)
            .unwrap_or("")
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    pub fn input_right(&mut self) {
        if self.cursor >= self.input_buffer.len() {
            return;
        }
        self.cursor = self
            .input_buffer
            .get(self.cursor..)
            .unwrap_or("")
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.input_buffer.len());
    }

    pub fn take_input(&mut self) -> String {
        let out = std::mem::take(&mut self.input_buffer);
        self.cursor = 0;
        out
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }
}
