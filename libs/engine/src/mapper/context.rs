//! `ContextReducer` — Article XIII rule 1 enforcement seam.
//!
//! Pattern: refs/stakpak/libs/agent-core/src/context.rs:8-17 (trait
//! definition) + :19-47 (`DefaultContextReducer`). The Stakpak version
//! takes 5 parameters (`messages`, `model`, `max_output_tokens`,
//! `tools`, `metadata`) — we narrow Stage 1 to just `messages` because
//! the additional context only earns its keep when
//! `BudgetAwareContextReducer` ships in S9.
//!
//! When S9 widens the trait, this module's signature MUST change in
//! lockstep — additive (extra params) is fine, removing the existing
//! `messages` parameter is a breaking change all callers feel.
//!
//! Constitution: Article XIII rule 1 — every Mapper run goes through
//! `reducer.reduce(messages)` BEFORE the LLM call. The Mapper's
//! `map(...)` function signature carries `&dyn ContextReducer`, so
//! the type system makes bypass impossible.
//!
//! Stage 1 ships `PassthroughContextReducer` — the Stakpak analog is
//! `PassthroughCompactionEngine` at refs/stakpak/libs/agent-core/src/compaction.rs:22-45.

use serde::{Deserialize, Serialize};

/// One conversation message. Stage 1 minimal shape — when the Mapper
/// path eventually evolves to use stakai's typed `Message` directly
/// (S5+), this local definition either disappears or grows a `From`
/// impl. Article XIII rule 4: stakai's `Message`/`Role` are the
/// canonical types; this is a Stage-1-local convenience that does NOT
/// redefine ChatMessage/LLMMessage from `libs/shared`.
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
}

/// Synchronous transform on a message list. Stage 1: the Passthrough
/// impl just returns the input unchanged. Stage 2+ may swap a
/// `BudgetAwareContextReducer` that drops oldest tool results, dedups
/// adjacent same-role messages, etc. — see Stakpak's
/// `reduce_context()` for the reference six-pass pipeline.
pub trait ContextReducer: Send + Sync {
    fn reduce(&self, messages: Vec<Message>) -> Vec<Message>;
}

/// Stage 1 default — returns input unchanged. Article XIII rule 1 is
/// satisfied by *being on the path*, not by mutation. When real
/// reducers ship (S9), the Mapper code stays untouched.
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
