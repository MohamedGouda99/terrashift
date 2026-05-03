//! `CompactionEngine` — overflow-recovery seam.
//!
//! Pattern: stakpak_arch.md §39 row 4 (TERRASHIFT_MAPPING.md §A row 4
//! canonical location: `libs/agent-core/src/compaction.rs`).
//! Source: refs/stakpak/libs/agent-core/src/compaction.rs:1-45
//! (trait + `PassthroughCompactionEngine`).
//!
//! Stage 1 ships the seam with the Passthrough impl. Real compaction
//! (drop-oldest-tool-results / dedup-adjacent-same-role / etc.)
//! arrives in S9 alongside the agent-loop kernel.
//!
//! ## Why ship the seam now
//! Per `refs/stakpak/libs/agent-core/src/agent.rs:193-219`, compaction
//! fires only on context-overflow errors during the agent loop —
//! *not* on the happy path. Stage 1's deterministic pipeline doesn't
//! invoke `compact()` at all (P-05 Mapper is a single LLM call; no
//! loop), but the trait must exist so:
//! 1. The agent loop kernel (S9 / `run_agent`) compiles against a
//!    type-system-stable seam.
//! 2. Stage 2 `Recovery` agent (S10) and `Cost Optimizer` agent (S11)
//!    can swap in real compactors without touching the kernel.
//!
//! Constitution: Article XII rule 2 (cache-stability — non-monotonic
//! trim boundaries break Anthropic prompt-cache hits, per Article XIII
//! rule 2; `PassthroughCompactionEngine` is trivially monotonic).

use crate::error::AgentError;
use async_trait::async_trait;

/// What a `compact()` call returns.
///
/// Mirrors `refs/stakpak/libs/agent-core/src/compaction.rs:5-11` shape.
/// `tokens_before` / `tokens_after` are advisory; Stage 1's Passthrough
/// reports them as equal (no truncation occurred). `truncated` flips
/// to `true` only when a real compactor actually drops messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionResult {
    pub messages: Vec<crate::context::Message>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub truncated: bool,
}

/// The async trait Stakpak's agent loop calls when context overflows.
/// Stage 1 Terrashift uses the same shape; Stage 1 doesn't actually
/// invoke it (no agent loop yet) but the seam is ready.
///
/// **Stage 1 narrowing**: Stakpak's `compact(messages, model)` carries
/// `&Model` so a budget-aware impl can compute per-model token budgets.
/// Terrashift's Stage 1 `PassthroughCompactionEngine` ignores the
/// argument; the seam is still there for S9. We use `crate::context::Message`
/// instead of `stakai::Message` because `LlmClient::complete` Stage 1
/// hasn't migrated to typed messages yet (Article XIII rule 4 boundary
/// stays at `libs/shared` when it lands).
#[async_trait]
pub trait CompactionEngine: Send + Sync {
    async fn compact(
        &self,
        messages: Vec<crate::context::Message>,
    ) -> Result<CompactionResult, AgentError>;
}

/// Stage 1 default — `Passthrough`, mirrors
/// `refs/stakpak/libs/agent-core/src/compaction.rs:22-45`.
/// Returns input unchanged with token-counts derived from word-count
/// proxy (no real tokenizer until S5+ when stakai's tokenizer is
/// accessible without a model dependency).
#[derive(Debug, Default)]
pub struct PassthroughCompactionEngine;

#[async_trait]
impl CompactionEngine for PassthroughCompactionEngine {
    async fn compact(
        &self,
        messages: Vec<crate::context::Message>,
    ) -> Result<CompactionResult, AgentError> {
        // Word-count proxy for token-count. Real tokenizer in S5+.
        let token_like_count: usize = messages
            .iter()
            .map(|m| m.content.split_whitespace().count())
            .sum();

        Ok(CompactionResult {
            messages,
            tokens_before: token_like_count,
            tokens_after: token_like_count,
            truncated: false,
        })
    }
}
