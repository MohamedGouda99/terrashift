//! TUI mainloop — `start_tui()` entry point.
//!
//! Pattern: refs/stakpak/tui/src/event_loop.rs (narrowed for Stage 2:
//! no mouse, no side panel, no banner overlays, no streaming agent
//! events. Synchronous command dispatch within the loop.)

use crate::app::{AppState, MessageKind, StatusInfo};
use crate::commands::{Action, CommandOutcome, Registry};
use crate::view::view;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::time::Duration;

/// Restores the terminal to a sane state on drop, even if the mainloop
/// panics. Mirrors `refs/stakpak/tui/src/terminal.rs::TerminalGuard`.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

/// Public entry — call from `cli/src/main.rs` when the operator invokes
/// `terrashift` with no subcommand.
pub async fn start_tui(status: StatusInfo) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let registry = Registry::stage1();
    let mut app = AppState::new(status);
    push_welcome(&mut app);

    loop {
        terminal.draw(|f| view(f, &app))?;

        if app.should_exit {
            break;
        }

        // Poll for an event with a short timeout so the UI stays responsive
        // (e.g., scroll redraws don't lag if nothing else is happening).
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                handle_key(key.code, key.modifiers, &mut app, &registry);
            }
            Event::Resize(_, _) => {
                // ratatui handles re-layout on next draw automatically.
            }
            _ => {}
        }
    }

    Ok(())
}

fn push_welcome(app: &mut AppState) {
    app.push(
        MessageKind::Banner,
        "Welcome. Try /help to list commands, /scan <dir> to inspect a Terraform tree.",
    );
    app.push(
        MessageKind::Banner,
        "For full migrations with real LLM, run `terrashift migrate ...` from the shell \
         (the in-TUI agent runtime arrives in S6).",
    );
    app.push(MessageKind::Banner, "Press Ctrl+C or type /quit to exit.");
}

fn handle_key(code: KeyCode, modifiers: KeyModifiers, app: &mut AppState, registry: &Registry) {
    // Ctrl+C → exit.
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        app.request_exit();
        return;
    }
    // Ctrl+D on empty input → exit.
    if modifiers.contains(KeyModifiers::CONTROL)
        && code == KeyCode::Char('d')
        && app.input_buffer.is_empty()
    {
        app.request_exit();
        return;
    }

    match code {
        KeyCode::Enter => {
            let line = app.take_input();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return;
            }
            app.push_user_input(trimmed);
            dispatch(trimmed, app, registry);
        }
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Delete => app.input_delete(),
        KeyCode::Left => app.input_left(),
        KeyCode::Right => app.input_right(),
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input_buffer.len(),
        KeyCode::PageUp => app.scroll_up(5),
        KeyCode::PageDown => app.scroll_down(5),
        KeyCode::Char(c) => app.input_insert(c),
        KeyCode::Esc => {
            if !app.input_buffer.is_empty() {
                let _ = app.take_input();
            } else {
                app.request_exit();
            }
        }
        _ => {}
    }
}

fn dispatch(input: &str, app: &mut AppState, registry: &Registry) {
    let outcome = if input.starts_with('/') {
        registry.dispatch(input)
    } else {
        // Bare text without slash — be friendly: redirect to /help.
        CommandOutcome::Hint(
            "Commands start with `/` (e.g., /help, /scan <dir>). \
             Type /help to list them, or run `terrashift migrate ...` from the shell."
                .to_string(),
        )
    };

    match outcome {
        CommandOutcome::Text(s) => app.push(MessageKind::Output, s),
        CommandOutcome::Error(s) => app.push(MessageKind::Error, s),
        CommandOutcome::Hint(s) => app.push(MessageKind::Hint, s),
        CommandOutcome::Action(action) => match action {
            Action::Exit => app.request_exit(),
            Action::StartMigration { plan_path } => {
                app.push(
                    MessageKind::Hint,
                    format!(
                        "/migrate {plan_path}: the in-TUI agent runtime is S6 work. \
                         For now, run `terrashift migrate --source <dir> --from <p> --to <p>` \
                         from the shell — that wires the full pipeline (Scanner → Mapper → \
                         Generator) with a real LLM call."
                    ),
                );
            }
            Action::Checkpoint => app.push(
                MessageKind::Hint,
                "/checkpoint: in-TUI checkpoint runtime is S6 work.".to_string(),
            ),
            Action::Rollback { run_id } => app.push(
                MessageKind::Hint,
                format!("/rollback {run_id}: in-TUI rollback runtime is S6 work."),
            ),
            Action::Compact => app.push(
                MessageKind::Hint,
                "/compact: in-TUI message compaction is S6 work.".to_string(),
            ),
        },
    }
}
