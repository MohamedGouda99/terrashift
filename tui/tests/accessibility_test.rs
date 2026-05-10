// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Accessibility scope: "screen-reader unhostile."
//! - No info conveyed by color alone (every coloured cell has a text label nearby).
//! - Layout-stability snapshot for the welcome screen.
//!
//! Spec: docs/superpowers/specs/2026-05-10-extended-testing-design.md §5.3.
//!
//! Implementation note: rather than introducing a new `render_welcome_to_buffer`
//! shim in the public API surface, this test reuses the existing
//! `view::view(f, &app)` rendering path with ratatui's `TestBackend` — the same
//! pattern already established in `view_render_test.rs`. The "welcome screen"
//! is defined as the rendered output of a freshly-constructed `AppState` with a
//! default `StatusInfo` — i.e. exactly what an operator sees when they launch
//! the TUI for the first time before typing anything.

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Terminal;
use terrashift_tui::{view, AppState, StatusInfo};

const TEST_AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// Renders the welcome screen (initial `AppState`) to an in-memory buffer.
/// Matches the rendering path in `view_render_test.rs` — `Terminal<TestBackend>`
/// then `terminal.draw(|f| view::view(f, &app))`.
fn render_welcome() -> Buffer {
    let backend = TestBackend::new(TEST_AREA.width, TEST_AREA.height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    let app = AppState::new(StatusInfo::default());
    terminal
        .draw(|f| view::view(f, &app))
        .expect("draw welcome");
    terminal.backend().buffer().clone()
}

fn buffer_to_text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in buf.area.top()..buf.area.bottom() {
        for x in buf.area.left()..buf.area.right() {
            let cell = buf.cell((x, y)).expect("cell in buffer area");
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn no_info_conveyed_by_color_alone() {
    let buf = render_welcome();
    for y in buf.area.top()..buf.area.bottom() {
        let mut row_has_text = false;
        let mut row_has_colored_cell = false;
        for x in buf.area.left()..buf.area.right() {
            let cell = buf.cell((x, y)).expect("cell in buffer area");
            if !cell.symbol().trim().is_empty() {
                row_has_text = true;
            }
            // Color::Reset == default terminal foreground; any other variant
            // means the cell has a non-default colour applied.
            if cell.fg != Color::Reset {
                row_has_colored_cell = true;
            }
        }
        if row_has_colored_cell {
            assert!(
                row_has_text,
                "row y={y} has coloured cells but no text — info conveyed by colour alone"
            );
        }
    }
}

#[test]
fn welcome_screen_matches_snapshot() {
    let buf = render_welcome();
    let actual = buffer_to_text(&buf);
    let snapshot_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join("accessibility")
        .join("welcome.txt");

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::create_dir_all(snapshot_path.parent().unwrap()).expect("mkdir snapshots");
        std::fs::write(&snapshot_path, &actual).expect("write snapshot");
        return;
    }

    let expected = std::fs::read_to_string(&snapshot_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", snapshot_path.display()));
    assert_eq!(
        actual, expected,
        "welcome screen drifted; review the diff and run with UPDATE_SNAPSHOTS=1 if intentional"
    );
}

#[test]
fn canary_fails_when_snapshot_missing() {
    let snap_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join("accessibility");
    let entries: Vec<_> = std::fs::read_dir(&snap_dir)
        .expect("read snap dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().into_string().unwrap_or_default())
        .filter(|n| !n.starts_with('.'))
        .collect();
    let expected: std::collections::BTreeSet<&str> = ["welcome.txt"].iter().copied().collect();
    let actual: std::collections::BTreeSet<&str> = entries.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        actual, expected,
        "snapshot directory contents drifted (expected = checked-in set)"
    );
}
