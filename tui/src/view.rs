// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! TUI rendering — single `view()` entry that draws every frame.
//!
//! Layout (top → bottom):
//!   1. Banner (1 row + borders) — "🚀 Terrashift v0.1.0  •  cross-cloud Terraform migration"
//!   2. Messages (flex; scrollable)
//!   3. Input box (3 rows: borders + prompt line)
//!   4. Status footer (1 row, no borders)

use crate::app::{AppState, MessageKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

const BANNER_HEIGHT: u16 = 3;
const INPUT_HEIGHT: u16 = 3;
const STATUS_HEIGHT: u16 = 1;

/// Single-shot render of `app` into `f`. Called every frame by the
/// event loop; `app` owns all the state, this fn just lays it out.
pub fn view(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(BANNER_HEIGHT),
            Constraint::Min(3),
            Constraint::Length(INPUT_HEIGHT),
            Constraint::Length(STATUS_HEIGHT),
        ])
        .split(f.area());

    render_banner(f, chunks[0]);
    render_messages(f, chunks[1], app);
    render_input(f, chunks[2], app);
    render_status(f, chunks[3], app);
}

fn render_banner(f: &mut Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");
    let title = Line::from(vec![
        Span::styled(
            "🚀 Terrashift",
            Style::default()
                .fg(Color::Rgb(0xFF, 0x7A, 0x45))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  v"),
        Span::styled(version, Style::default().fg(Color::Rgb(0xCE, 0x42, 0x2B))),
        Span::raw("  •  "),
        Span::styled(
            "cross-cloud Terraform migration",
            Style::default().fg(Color::Rgb(0x94, 0xA3, 0xB8)),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(0xCE, 0x42, 0x2B)));
    let p = Paragraph::new(title)
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(p, area);
}

fn render_messages(f: &mut Frame, area: Rect, app: &AppState) {
    let lines: Vec<Line> = build_message_lines(&app.messages);

    // Apply scroll offset (0 = pinned to bottom).
    let visible_height = area.height.saturating_sub(2) as usize; // -2 for borders
    let total = lines.len();
    let start = total
        .saturating_sub(visible_height + app.scroll_offset)
        .min(total.saturating_sub(visible_height.min(total)));

    let visible: Vec<Line> = lines.into_iter().skip(start).take(visible_height).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(0x4B, 0x55, 0x63)));

    let p = Paragraph::new(visible)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn build_message_lines(messages: &[crate::app::Message]) -> Vec<Line<'static>> {
    let mut out: Vec<Line> = Vec::with_capacity(messages.len() * 2);
    for msg in messages {
        let style = match msg.kind {
            MessageKind::Banner => Style::default().fg(Color::Rgb(0x94, 0xA3, 0xB8)),
            MessageKind::UserInput => Style::default()
                .fg(Color::Rgb(0xFF, 0x7A, 0x45))
                .add_modifier(Modifier::BOLD),
            MessageKind::Output => Style::default().fg(Color::Rgb(0xE5, 0xE7, 0xEB)),
            MessageKind::Error => Style::default().fg(Color::Rgb(0xEF, 0x44, 0x44)),
            MessageKind::Hint => Style::default()
                .fg(Color::Rgb(0xFB, 0xBF, 0x24))
                .add_modifier(Modifier::ITALIC),
        };
        for line in msg.text.split('\n') {
            out.push(Line::from(Span::styled(line.to_string(), style)));
        }
        // Blank separator between messages.
        out.push(Line::from(""));
    }
    out
}

fn render_input(f: &mut Frame, area: Rect, app: &AppState) {
    let prompt = Line::from(vec![
        Span::styled(
            "▶ ",
            Style::default()
                .fg(Color::Rgb(0xFF, 0x7A, 0x45))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.input_buffer.clone()),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(0xFF, 0x7A, 0x45)));
    let p = Paragraph::new(prompt).block(block);
    f.render_widget(p, area);

    // Position the cursor inside the input area.
    let cursor_x = area.x + 3 + app.cursor as u16; // 1 (border) + 2 ("▶ ")
    let cursor_y = area.y + 1;
    if cursor_x < area.x + area.width.saturating_sub(1) {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_status(f: &mut Frame, area: Rect, app: &AppState) {
    let profile = app
        .status
        .profile_path
        .as_ref()
        .map(|p| {
            let s = p.display().to_string();
            // Compress the home prefix so the footer stays readable.
            if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
                if s.starts_with(&home) {
                    return s.replacen(&home, "~", 1);
                }
            }
            s
        })
        .unwrap_or_else(|| "(no profile)".to_string());

    let seed = match app.status.seed_resources {
        Some(n) => format!("{n} resources"),
        None => "not loaded".to_string(),
    };

    let schemas = match app.status.cached_schema_count {
        Some(n) => format!("{n} cached"),
        None => "not loaded".to_string(),
    };

    let line = Line::from(vec![
        Span::styled(
            "  profile: ",
            Style::default().fg(Color::Rgb(0x6B, 0x72, 0x80)),
        ),
        Span::styled(profile, Style::default().fg(Color::Rgb(0x94, 0xA3, 0xB8))),
        Span::styled(
            " │ seed: ",
            Style::default().fg(Color::Rgb(0x6B, 0x72, 0x80)),
        ),
        Span::styled(seed, Style::default().fg(Color::Rgb(0x94, 0xA3, 0xB8))),
        Span::styled(
            " │ schemas: ",
            Style::default().fg(Color::Rgb(0x6B, 0x72, 0x80)),
        ),
        Span::styled(schemas, Style::default().fg(Color::Rgb(0x94, 0xA3, 0xB8))),
        Span::styled(
            " │ tier: ",
            Style::default().fg(Color::Rgb(0x6B, 0x72, 0x80)),
        ),
        Span::styled(
            app.status.tier.clone(),
            Style::default().fg(Color::Rgb(0x94, 0xA3, 0xB8)),
        ),
        Span::styled(
            " │ Ctrl+C to exit ",
            Style::default()
                .fg(Color::Rgb(0x6B, 0x72, 0x80))
                .add_modifier(Modifier::ITALIC),
        ),
    ]);
    let p = Paragraph::new(line).alignment(Alignment::Left);
    f.render_widget(p, area);
}
