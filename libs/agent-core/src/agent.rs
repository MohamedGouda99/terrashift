//! `run_agent` — the canonical Terrashift agent loop kernel (S9).
//!
//! Pattern: stakpak_arch.md §8 (THE CANONICAL AGENT LOOP).
//! Source: refs/stakpak/libs/agent-core/src/agent.rs (859 lines, narrowed
//! for Stage 2 — see below).
//!
//! Constitution:
//! - Article I (bounded blast radius — kernel exists; agents that use it
//!   are gated behind explicit RFC + stage-gate per Article I).
//! - Article IV (loud errors — every termination reason is an explicit
//!   `AgentLoopReason` or `AgentError`; no silent stalls).
//! - Article XIII rule 1 (ContextReducer on the LLM-call path).
//! - Article XIII rule 9 (no duplicate tool_call_id within a turn).
//!
//! ## Stage 2 narrowing vs Stakpak's run_agent
//!
//! What we keep:
//! - Multi-turn loop with bounded `max_turns`
//! - `ApprovalStateMachine` for per-turn ordered tool dispatch
//! - Retry on transient LLM errors (header-driven + exponential)
//! - All 5 `AgentHook` lifecycle methods fired in canonical order
//! - `ContextReducer::reduce` before every LLM call
//! - `CompactionEngine::compact` when token threshold crossed
//!
//! What we deliberately defer:
//! - Streaming output (`stream.rs`) — no Stage 2 consumer streams; S5+ adds.
//! - Interactive steering channels (Steer, FollowUp, SwitchModel, Abort)
//!   — S13 detached mode adds these.
//! - Tokenizer-driven token counts — Stage 2 uses word-count proxy from
//!   `PassthroughCompactionEngine`; S5+ wires stakai's tokenizer.
//!
//! Stage 1 has zero agents (Article I), so this kernel is structurally
//! inert until S10's Recovery agent activates it. The kernel compiles +
//! tests pass; no production-path code in the workspace calls
//! `run_agent` until that session.

use crate::approval::ApprovalStateMachine;
use crate::compaction::CompactionEngine;
use crate::context::{ContextReducer, Message, Role};
use crate::error::AgentError;
use crate::hooks::AgentHook;
use crate::retry::resolve_retry_delay_ms;
use crate::tools::{ToolExecutionResult, ToolExecutor};
use crate::types::{
    AgentLoopConfig, AgentLoopReason, AgentLoopResult, AgentRunContext, ProposedToolCall,
    RetryConfig, ToolDecision,
};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------
// Public types — agent message thread + LLM contract
// ---------------------------------------------------------------------

/// One conversation message in the kernel's internal thread. Extends the
/// Stage-1 `Message` (role + content) with tool-call metadata so the
/// thread carries enough state for the LLM to follow tool/result pairs.
///
/// When passing the thread to `ContextReducer::reduce` or
/// `CompactionEngine::compact`, the kernel projects to the simpler
/// Stage-1 `Message` (lossy on `tool_call_id` and `tool_calls`). In
/// Stage 2 the passthrough impls don't drop messages so the round-trip
/// preserves the kernel's thread by index. S5+ widens the seam to take
/// `AgentMessage` directly when budget-aware reducers ship.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentMessage {
    pub role: Role,
    pub content: String,
    /// Set on `Role::Tool` messages — links the tool result back to the
    /// originating `ProposedToolCall::id`. `None` for system / user /
    /// assistant messages.
    pub tool_call_id: Option<String>,
    /// Set on `Role::Assistant` messages when the LLM emitted tool
    /// calls. The kernel populates this from the `LlmTurnOutcome`.
    pub tool_calls: Vec<ProposedToolCall>,
}

impl AgentMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ProposedToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

/// Tool definition the kernel sends to the LLM in each turn. Stage 2
/// narrowing: just the three fields every provider needs (name,
/// description, JSON schema). S5+ extends with input/output shape +
/// permission metadata when MCP tool-router widening lands.
#[derive(Debug, Clone)]
pub struct AgentToolDef {
    pub name: String,
    pub description: String,
    pub schema: serde_json::Value,
}

/// What the LLM produced in one turn.
#[derive(Debug, Clone, Default)]
pub struct LlmTurnOutcome {
    /// The LLM's final answer text. `Some` when no tool calls were
    /// proposed (terminal turn) or when the model produced both
    /// reasoning text + tool calls. `None` when the model returned
    /// only tool calls.
    pub final_text: Option<String>,
    pub proposed_tool_calls: Vec<ProposedToolCall>,
    pub usage_input_tokens: u64,
    pub usage_output_tokens: u64,
}

/// LLM call error — distinguishes retryable (rate limit, 5xx, timeout)
/// from non-retryable (auth, schema mismatch, model unavailable).
#[derive(Debug, Clone)]
pub struct LlmTurnError {
    pub message: String,
    /// `true` → kernel will retry per `RetryConfig`. `false` → kernel
    /// returns `AgentError::Inference` immediately.
    pub retryable: bool,
    /// HTTP response headers — used by `retry::resolve_retry_delay_ms`
    /// to honor `retry-after` / `retry-after-ms`. Empty when the
    /// implementation has no header context.
    pub headers: HashMap<String, String>,
}

/// The agent kernel's LLM contract. Decoupled from `libs/ai`'s
/// `LlmClient` because the kernel needs:
/// 1. Full message threading (the `LlmClient::complete` Stage-1 API
///    only takes a single string).
/// 2. Tool-call propagation (proposed calls back to the kernel).
/// 3. Retry-header surface (so retry-after parsing works).
///
/// S10 (Recovery agent) will provide the first concrete impl —
/// likely a thin adapter over `stakai::Inference` that converts
/// between `AgentMessage` and `stakai::Message`.
#[async_trait]
pub trait AgentLlmClient: Send + Sync {
    async fn generate_turn(
        &self,
        ctx: &AgentRunContext,
        messages: &[AgentMessage],
        tools: &[AgentToolDef],
    ) -> Result<LlmTurnOutcome, LlmTurnError>;
}

// ---------------------------------------------------------------------
// Public entry point — run_agent
// ---------------------------------------------------------------------

/// The canonical agent loop. Stage 2 entry point.
///
/// Lifecycle:
///
/// 1. Validate config (`max_turns > 0`).
/// 2. For each turn `1..=max_turns`:
///    1. Run `reducer.reduce()` on the message thread.
///    2. If cumulative input tokens exceed `compaction_threshold_tokens`,
///       run `compactor.compact()`.
///    3. Fire `before_inference` on every hook.
///    4. Call `llm.generate_turn()` with retry per `RetryConfig`.
///    5. Fire `after_inference` on every hook.
///    6. If LLM returned final answer (no tool calls), return
///       `AgentLoopResult { reason: FinalAnswerReached }`.
///    7. Validate no duplicate `tool_call_id` (Article XIII rule 9).
///    8. Build `ApprovalStateMachine` from policy. If any call is
///       `PendingUserDecision`, return
///       `AgentLoopResult { reason: WaitingForApproval }`.
///    9. Drain accepted/rejected calls in proposed order, firing
///       `before_tool_execution` / `after_tool_execution` around each.
/// 3. If we exit the loop without returning, `max_turns` was reached
///    → `AgentLoopResult { reason: MaxTurnsReached }`.
///
/// Errors propagate up via `AgentError`. `Cancelled` from the
/// `CancellationToken` is a clean stop — caller chooses whether to
/// surface it as an error.
#[allow(clippy::too_many_arguments)] // canonical Stakpak shape; keep parity
pub async fn run_agent(
    config: &AgentLoopConfig,
    ctx: &AgentRunContext,
    initial_messages: Vec<AgentMessage>,
    tools: &[AgentToolDef],
    executor: &dyn ToolExecutor,
    hooks: &[Box<dyn AgentHook>],
    llm: &dyn AgentLlmClient,
    reducer: &dyn ContextReducer,
    compactor: &dyn CompactionEngine,
    cancel: &CancellationToken,
) -> Result<AgentLoopResult, AgentError> {
    if config.max_turns == 0 {
        return Err(AgentError::InvalidConfig(
            "max_turns must be > 0".to_string(),
        ));
    }

    let mut messages: Vec<AgentMessage> = initial_messages;
    let mut total_input_tokens: u64 = 0;

    for turn in 1..=config.max_turns {
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        // 1. Reducer (Article XIII rule 1 enforcement seam).
        messages = run_reducer(reducer, messages);

        // 2. Compaction trigger — only fires above threshold.
        if u64::try_from(config.compaction_threshold_tokens)
            .map(|t| total_input_tokens > t)
            .unwrap_or(false)
        {
            messages = run_compactor(compactor, messages).await?;
        }

        // 3. Hooks: before_inference. The hook signature takes
        //    `&[stakai::Message]` + `&stakai::Model`; project from
        //    AgentMessage.
        let stakai_messages = to_stakai_messages(&messages);
        let stakai_model = stakai::Model::default();
        for hook in hooks {
            hook.before_inference(ctx, &stakai_messages, &stakai_model)
                .await?;
        }

        // 4. LLM call with retry.
        let outcome = call_llm_with_retry(llm, ctx, &messages, tools, &config.retry).await?;
        total_input_tokens = total_input_tokens.saturating_add(outcome.usage_input_tokens);

        // 5. Hooks: after_inference (using a fresh stakai_messages
        //    snapshot — the inference itself didn't mutate messages,
        //    so we can reuse).
        for hook in hooks {
            hook.after_inference(ctx, &stakai_messages, &stakai_model)
                .await?;
        }

        // 6. Final-answer fast path — no tool calls means the LLM is
        //    done thinking. Append assistant message, return.
        if outcome.proposed_tool_calls.is_empty() {
            let final_text = outcome.final_text.unwrap_or_default();
            messages.push(AgentMessage::assistant(final_text.clone(), Vec::new()));
            return Ok(AgentLoopResult {
                reason: AgentLoopReason::FinalAnswerReached,
                final_text,
                turns_executed: turn,
                pending_tool_calls: Vec::new(),
            });
        }

        // 7. Article XIII rule 9: no duplicate tool_call_id within a
        //    single turn. Buggy provider responses can violate this;
        //    fail loud.
        let mut seen: HashSet<&str> = HashSet::new();
        for tc in &outcome.proposed_tool_calls {
            if !seen.insert(tc.id.as_str()) {
                return Err(AgentError::DuplicateToolCallId {
                    tool_call_id: tc.id.clone(),
                });
            }
        }

        // 8. Append the LLM's assistant turn (with tool calls) to the
        //    thread so subsequent reducer/compactor passes see it.
        messages.push(AgentMessage::assistant(
            outcome.final_text.clone().unwrap_or_default(),
            outcome.proposed_tool_calls.clone(),
        ));

        // 9. Approval state machine — per-turn-fresh.
        let mut approvals =
            ApprovalStateMachine::new(outcome.proposed_tool_calls.clone(), &config.approval_policy);

        if approvals.is_waiting_for_user() {
            return Ok(AgentLoopResult {
                reason: AgentLoopReason::WaitingForApproval,
                final_text: String::new(),
                turns_executed: turn,
                pending_tool_calls: approvals.pending_tool_call_ids(),
            });
        }

        // 10. Drain ready calls in proposed order.
        while let Some(resolved) = approvals.next_ready() {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }

            let tc = resolved.tool_call;

            // Hook: before_tool_execution
            for hook in hooks {
                hook.before_tool_execution(ctx, &tc, &stakai_messages)
                    .await?;
            }

            let result_content = match resolved.decision {
                ToolDecision::Accept => {
                    let exec_result = executor.execute_tool_call(ctx, &tc, cancel).await?;
                    match exec_result {
                        ToolExecutionResult::Completed { result, is_error } => {
                            if is_error {
                                format!("[tool_error] {result}")
                            } else {
                                result
                            }
                        }
                        ToolExecutionResult::Cancelled => return Err(AgentError::Cancelled),
                    }
                }
                ToolDecision::Reject => {
                    format!("Tool call rejected by approval policy: {}", tc.name)
                }
                ToolDecision::CustomResult { content } => content,
            };

            messages.push(AgentMessage::tool_result(&tc.id, result_content));

            // Hook: after_tool_execution
            for hook in hooks {
                hook.after_tool_execution(ctx, &tc, &stakai_messages)
                    .await?;
            }
        }

        // Continue to next turn.
    }

    // max_turns reached — return the last assistant text, if any.
    let final_text = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant)
        .map(|m| m.content.clone())
        .unwrap_or_default();

    Ok(AgentLoopResult {
        reason: AgentLoopReason::MaxTurnsReached,
        final_text,
        turns_executed: config.max_turns,
        pending_tool_calls: Vec::new(),
    })
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Project AgentMessage → Vec<Message>, run the reducer, project back.
///
/// In Stage 2 the PassthroughContextReducer doesn't drop messages, so
/// the round-trip is lossless (kernel keeps the rich AgentMessage
/// thread). When a real reducer ships in S5+ that *does* drop messages,
/// we'll match by (role, content) order to rebuild the AgentMessage
/// thread; if a real reducer mutates content the rebuild becomes
/// imprecise — that's the trigger to widen the trait surface to take
/// AgentMessage directly.
fn run_reducer(reducer: &dyn ContextReducer, messages: Vec<AgentMessage>) -> Vec<AgentMessage> {
    let projected: Vec<Message> = messages
        .iter()
        .map(|m| Message {
            role: m.role,
            content: m.content.clone(),
        })
        .collect();

    let reduced = reducer.reduce(projected);

    if reduced.len() == messages.len() {
        // Fast path — most common in Stage 2.
        return messages;
    }

    rebuild_thread_after_drop(messages, &reduced)
}

async fn run_compactor(
    compactor: &dyn CompactionEngine,
    messages: Vec<AgentMessage>,
) -> Result<Vec<AgentMessage>, AgentError> {
    let projected: Vec<Message> = messages
        .iter()
        .map(|m| Message {
            role: m.role,
            content: m.content.clone(),
        })
        .collect();

    let result = compactor.compact(projected).await?;

    if result.messages.len() == messages.len() {
        return Ok(messages);
    }

    Ok(rebuild_thread_after_drop(messages, &result.messages))
}

/// Walk both threads in order and keep the AgentMessages whose
/// (role, content) matches a corresponding entry in the projected
/// reduced thread. Stage 2 dead code (passthrough impls don't drop);
/// Stage 5+ insurance.
fn rebuild_thread_after_drop(
    original: Vec<AgentMessage>,
    reduced: &[Message],
) -> Vec<AgentMessage> {
    let mut rebuilt: Vec<AgentMessage> = Vec::with_capacity(reduced.len());
    let mut iter = original.into_iter();
    for r in reduced {
        for m in iter.by_ref() {
            if m.role == r.role && m.content == r.content {
                rebuilt.push(m);
                break;
            }
        }
    }
    rebuilt
}

async fn call_llm_with_retry(
    llm: &dyn AgentLlmClient,
    ctx: &AgentRunContext,
    messages: &[AgentMessage],
    tools: &[AgentToolDef],
    retry: &RetryConfig,
) -> Result<LlmTurnOutcome, AgentError> {
    let mut last_error_message = String::new();

    for attempt in 1..=retry.max_attempts {
        match llm.generate_turn(ctx, messages, tools).await {
            Ok(outcome) => return Ok(outcome),
            Err(err) => {
                last_error_message = err.message.clone();
                if !err.retryable || attempt == retry.max_attempts {
                    break;
                }
                let delay = resolve_retry_delay_ms(&err.headers, retry, attempt, Utc::now());
                sleep(Duration::from_millis(delay.delay_ms)).await;
            }
        }
    }

    Err(AgentError::LlmRetryExhausted {
        attempts: retry.max_attempts,
        last_error: last_error_message,
    })
}

fn to_stakai_messages(messages: &[AgentMessage]) -> Vec<stakai::Message> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => stakai::Role::System,
                Role::User => stakai::Role::User,
                Role::Assistant => stakai::Role::Assistant,
                Role::Tool => stakai::Role::Tool,
            };
            stakai::Message::new(role, m.content.clone())
        })
        .collect()
}
