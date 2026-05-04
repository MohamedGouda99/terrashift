// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `ContextReducer` — Article XIII rule 1 enforcement seam.
//!
//! Pattern: the architecture reference §39 row 5 (TERRASHIFT_MAPPING.md §A row 5
//! canonical location: `libs/agent-core/src/context.rs`).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (trait
//! definition) + :19-47 (`DefaultContextReducer`).
//!
//! Stage 1 narrows the trait signature: just `messages` instead of
//! the reference's 5-arg form (`messages, model, max_output_tokens, tools,
//! metadata`). The additional context only earns its keep when
//! `BudgetAwareContextReducer` ships in S9. When S9 widens this trait,
//! it MUST be additive — removing the existing `messages` parameter
//! is a breaking change every consumer feels.
//!
//! Constitution: Article XIII rule 1 — every Mapper / Recovery / Cost
//! Optimizer run that talks to the LLM goes through
//! `reducer.reduce(messages)` BEFORE the LLM call. The seam being on
//! the type signature (`&dyn ContextReducer` parameter) makes bypass
//! a compile-time impossibility.
//!
//! ## Stage 1 deliverable
//! - The `ContextReducer` trait
//! - `Message` + `Role` types (Stage-1-local; converge with stakai's
//!   typed Message in S5+ when LlmClient evolves to typed messages)
//! - `PassthroughContextReducer` — the reference's `Default` impl shape but
//!   with the narrowed signature

use serde::{Deserialize, Serialize};

/// One conversation message. Stage-1-local convenience type — when
/// `LlmClient` evolves (S5+) to use stakai's typed `Message`/`Role`
/// directly, this either disappears or grows a `From<stakai::Message>`
/// impl. Article XIII rule 4: stakai's types remain canonical; this
/// is NOT a redefinition of `ChatMessage`/`LLMMessage` from libs/shared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// Tool result message — added in S9 for the agent kernel's tool
    /// loop. Mapper (Stage 1, single-shot) does not produce Tool
    /// messages but must handle the variant for exhaustive matches.
    Tool,
}

/// Synchronous transform on a message list. Stage 1: the Passthrough
/// impl just returns the input unchanged — Article XIII rule 1 is
/// satisfied by *being on the path*, not by mutation. When real
/// reducers ship (S9 — `BudgetAwareContextReducer` per
/// the architecture reference §39 row 5 expansion), consumer code stays
/// untouched because the trait signature doesn't change.
pub trait ContextReducer: Send + Sync {
    fn reduce(&self, messages: Vec<Message>) -> Vec<Message>;
}

/// Stage 1 default. Mirrors `the reference codebase (see ATTRIBUTIONS.md)`'s
/// `DefaultContextReducer` shape (no-op transform). the reference's variant
/// runs a 6-pass pipeline; we narrow to no-op until BudgetAware lands.
pub struct PassthroughContextReducer;

impl ContextReducer for PassthroughContextReducer {
    fn reduce(&self, messages: Vec<Message>) -> Vec<Message> {
        messages
    }
}

impl Default for PassthroughContextReducer {
    fn default() -> Self {
        Self
    }
}
