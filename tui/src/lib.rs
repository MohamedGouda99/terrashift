// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Terrashift TUI — Ratatui-based interactive terminal UI.
//!
//! Pattern: the architecture reference §16 (TUI structure: AppState,
//! services/handlers, slash-command dispatch). Stage 2 narrowing —
//! synchronous command dispatch, no agent-runtime streaming, no mouse
//! capture, no banner overlays. Those land in S6.
//!
//! ## Modules
//! - `commands` — slash-command registry + per-file commands.
//! - `app`      — `AppState` (message history, input buffer, cursor, scroll).
//! - `view`     — single-frame ratatui rendering.
//! - `event_loop` — `start_tui()` mainloop entry point.

pub mod app;
pub mod commands;
pub mod event_loop;
pub mod view;

pub use app::{AppState, Message, MessageKind, StatusInfo};
pub use commands::Registry;
pub use event_loop::start_tui;
