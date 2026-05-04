// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! S9 — Agent loop kernel integration tests.
//!
//! Tests the `run_agent` happy path + edge cases via in-memory stubs:
//! - `ScriptedLlm` plays back canned `LlmTurnOutcome` per turn.
//! - `EchoExecutor` returns `tool_call.arguments` as the result.
//! - `RecordingHook` accumulates lifecycle phase names so we can
//!   assert the canonical hook-fire order.
//!
//! Pattern: the architecture reference §8 (agent loop kernel test contract).
//! Spec: specs/015-agent-loop-kernel/spec.md (User Stories 1-4).

use async_trait::async_trait;
use serde_json::json;
use stakai::{Message as StakMessage, Model as StakModel};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use terrashift_agent_core::agent::{
    run_agent, AgentLlmClient, AgentMessage, AgentToolDef, LlmTurnError, LlmTurnOutcome,
};
use terrashift_agent_core::{
    AgentError, AgentHook, AgentLoopConfig, AgentLoopReason, AgentRunContext,
    PassthroughCompactionEngine, PassthroughContextReducer, ProposedToolCall, RetryConfig,
    ToolApprovalPolicy, ToolExecutionResult, ToolExecutor,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ---------------------------------------------------------------------
// Stubs
// ---------------------------------------------------------------------

struct ScriptedLlm {
    /// Each entry is the outcome for turn N (0-indexed). When the call
    /// count exceeds `outcomes.len()`, the stub panics — tests must
    /// supply enough outcomes for the loop they exercise.
    outcomes: Vec<Result<LlmTurnOutcome, LlmTurnError>>,
    call_count: Mutex<usize>,
}

impl ScriptedLlm {
    fn new(outcomes: Vec<Result<LlmTurnOutcome, LlmTurnError>>) -> Self {
        Self {
            outcomes,
            call_count: Mutex::new(0),
        }
    }
}

#[async_trait]
impl AgentLlmClient for ScriptedLlm {
    async fn generate_turn(
        &self,
        _ctx: &AgentRunContext,
        _messages: &[AgentMessage],
        _tools: &[AgentToolDef],
    ) -> Result<LlmTurnOutcome, LlmTurnError> {
        let mut count = self.call_count.lock().expect("count mutex poisoned");
        let idx = *count;
        *count += 1;
        self.outcomes
            .get(idx)
            .cloned()
            .unwrap_or_else(|| panic!("ScriptedLlm exhausted (call #{idx})"))
    }
}

struct EchoExecutor;

#[async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute_tool_call(
        &self,
        _run: &AgentRunContext,
        tool_call: &ProposedToolCall,
        _cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        Ok(ToolExecutionResult::Completed {
            result: format!("echo:{}", tool_call.arguments),
            is_error: false,
        })
    }
}

#[derive(Default, Clone)]
struct RecordingHook {
    log: Arc<Mutex<Vec<String>>>,
}

impl RecordingHook {
    fn snapshot(&self) -> Vec<String> {
        self.log.lock().expect("recording hook mutex").clone()
    }

    fn push(&self, phase: &str) {
        self.log
            .lock()
            .expect("recording hook mutex")
            .push(phase.to_string());
    }
}

#[async_trait]
impl AgentHook for RecordingHook {
    async fn before_inference(
        &self,
        _run: &AgentRunContext,
        _messages: &[StakMessage],
        _model: &StakModel,
    ) -> Result<(), AgentError> {
        self.push("before_inference");
        Ok(())
    }

    async fn after_inference(
        &self,
        _run: &AgentRunContext,
        _messages: &[StakMessage],
        _model: &StakModel,
    ) -> Result<(), AgentError> {
        self.push("after_inference");
        Ok(())
    }

    async fn before_tool_execution(
        &self,
        _run: &AgentRunContext,
        _tool_call: &ProposedToolCall,
        _messages: &[StakMessage],
    ) -> Result<(), AgentError> {
        self.push("before_tool_execution");
        Ok(())
    }

    async fn after_tool_execution(
        &self,
        _run: &AgentRunContext,
        _tool_call: &ProposedToolCall,
        _messages: &[StakMessage],
    ) -> Result<(), AgentError> {
        self.push("after_tool_execution");
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

fn run_ctx() -> AgentRunContext {
    AgentRunContext {
        run_id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
    }
}

fn final_answer(text: &str) -> LlmTurnOutcome {
    LlmTurnOutcome {
        final_text: Some(text.to_string()),
        proposed_tool_calls: Vec::new(),
        usage_input_tokens: 0,
        usage_output_tokens: 0,
    }
}

fn tool_call(id: &str, name: &str) -> ProposedToolCall {
    ProposedToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: json!({"id": id}),
        metadata: None,
    }
}

fn turn_with_calls(calls: Vec<ProposedToolCall>) -> LlmTurnOutcome {
    LlmTurnOutcome {
        final_text: None,
        proposed_tool_calls: calls,
        usage_input_tokens: 0,
        usage_output_tokens: 0,
    }
}

fn config_for_turns(max_turns: usize, policy: ToolApprovalPolicy) -> AgentLoopConfig {
    AgentLoopConfig {
        max_turns,
        retry: RetryConfig::default(),
        approval_policy: policy,
        compaction_threshold_tokens: usize::MAX,
    }
}

// ---------------------------------------------------------------------
// User Story 1 — Run loop dispatches LLM-proposed tools
// ---------------------------------------------------------------------

#[tokio::test]
async fn us1_loop_completes_when_llm_returns_final_answer() {
    let llm = ScriptedLlm::new(vec![Ok(final_answer("done"))]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let result = run_agent(
        &config_for_turns(3, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("hello")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    assert_eq!(result.reason, AgentLoopReason::FinalAnswerReached);
    assert_eq!(result.final_text, "done");
    assert_eq!(result.turns_executed, 1);
}

#[tokio::test]
async fn us1_one_tool_call_then_final_answer() {
    let llm = ScriptedLlm::new(vec![
        Ok(turn_with_calls(vec![tool_call("tc_1", "echo")])),
        Ok(final_answer("after-tool")),
    ]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let result = run_agent(
        &config_for_turns(5, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    assert_eq!(result.reason, AgentLoopReason::FinalAnswerReached);
    assert_eq!(result.final_text, "after-tool");
    assert_eq!(result.turns_executed, 2);
}

#[tokio::test]
async fn us1_max_turns_reached_returns_loud_terminal_state() {
    // LLM keeps proposing tool calls forever — max_turns must clamp.
    let llm = ScriptedLlm::new(vec![
        Ok(turn_with_calls(vec![tool_call("tc_1", "echo")])),
        Ok(turn_with_calls(vec![tool_call("tc_2", "echo")])),
        Ok(turn_with_calls(vec![tool_call("tc_3", "echo")])),
    ]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let result = run_agent(
        &config_for_turns(3, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("loop")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    assert_eq!(result.reason, AgentLoopReason::MaxTurnsReached);
    assert_eq!(result.turns_executed, 3);
}

#[tokio::test]
async fn us1_tool_rejection_is_fed_back_to_llm() {
    let llm = ScriptedLlm::new(vec![
        Ok(turn_with_calls(vec![tool_call("tc_1", "danger")])),
        Ok(final_answer("after-rejection")),
    ]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let result = run_agent(
        &config_for_turns(5, ToolApprovalPolicy::DenyAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    // After a rejection, the loop continues and the LLM produces a final answer.
    assert_eq!(result.reason, AgentLoopReason::FinalAnswerReached);
    assert_eq!(result.final_text, "after-rejection");
    assert_eq!(result.turns_executed, 2);
}

// ---------------------------------------------------------------------
// User Story 2 — Approval gate covered by approval.rs's own unit tests.
// (See libs/agent-core/src/approval.rs#mod tests for the 7 ordering tests.)
//
// User Story 3 — Retry on transient LLM errors
// ---------------------------------------------------------------------

#[tokio::test]
async fn us3_retry_recovers_on_third_attempt() {
    let retryable_err = || LlmTurnError {
        message: "rate limit".to_string(),
        retryable: true,
        headers: HashMap::new(), // no header → exponential backoff
    };
    let llm = ScriptedLlm::new(vec![
        Err(retryable_err()),
        Err(retryable_err()),
        Ok(final_answer("recovered")),
    ]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    // Use a tiny initial backoff so the test runs fast.
    let config = AgentLoopConfig {
        max_turns: 2,
        retry: RetryConfig {
            max_attempts: 5,
            initial_backoff_ms: 1,
            max_backoff_ms: 10,
            multiplier: 2.0,
        },
        approval_policy: ToolApprovalPolicy::AcceptAll,
        compaction_threshold_tokens: usize::MAX,
    };

    let result = run_agent(
        &config,
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    assert_eq!(result.reason, AgentLoopReason::FinalAnswerReached);
    assert_eq!(result.final_text, "recovered");
}

#[tokio::test]
async fn us3_retry_exhausts_and_returns_loud_error() {
    let retryable_err = || LlmTurnError {
        message: "always fails".to_string(),
        retryable: true,
        headers: HashMap::new(),
    };
    let llm = ScriptedLlm::new(vec![
        Err(retryable_err()),
        Err(retryable_err()),
        Err(retryable_err()),
    ]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let config = AgentLoopConfig {
        max_turns: 1,
        retry: RetryConfig {
            max_attempts: 3,
            initial_backoff_ms: 1,
            max_backoff_ms: 5,
            multiplier: 2.0,
        },
        approval_policy: ToolApprovalPolicy::AcceptAll,
        compaction_threshold_tokens: usize::MAX,
    };

    let err = run_agent(
        &config,
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect_err("run_agent should fail");

    match err {
        AgentError::LlmRetryExhausted {
            attempts,
            last_error,
        } => {
            assert_eq!(attempts, 3);
            assert_eq!(last_error, "always fails");
        }
        other => panic!("expected LlmRetryExhausted, got {other:?}"),
    }
}

#[tokio::test]
async fn us3_non_retryable_error_does_not_retry() {
    let non_retryable_err = LlmTurnError {
        message: "auth failed".to_string(),
        retryable: false,
        headers: HashMap::new(),
    };
    let llm = ScriptedLlm::new(vec![Err(non_retryable_err)]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let err = run_agent(
        &config_for_turns(1, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect_err("run_agent should fail");

    // After 1 attempt the kernel returns LlmRetryExhausted with attempts=max_attempts (default 4).
    // The loop bails on first non-retryable; attempt count records max_attempts (the budget).
    assert!(matches!(err, AgentError::LlmRetryExhausted { .. }));
}

// ---------------------------------------------------------------------
// User Story 4 — Hooks fire at canonical points
// ---------------------------------------------------------------------

#[tokio::test]
async fn us4_hooks_fire_in_canonical_order() {
    let llm = ScriptedLlm::new(vec![
        Ok(turn_with_calls(vec![tool_call("tc_1", "echo")])),
        Ok(final_answer("done")),
    ]);
    let executor = EchoExecutor;
    let recording = RecordingHook::default();
    let hooks: Vec<Box<dyn AgentHook>> = vec![Box::new(recording.clone())];
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    run_agent(
        &config_for_turns(3, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    let log = recording.snapshot();
    let expected = vec![
        "before_inference",
        "after_inference",
        "before_tool_execution",
        "after_tool_execution",
        "before_inference",
        "after_inference",
    ];
    assert_eq!(
        log, expected,
        "hook fire order must match canonical sequence"
    );
}

// ---------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------

#[tokio::test]
async fn edge_invalid_max_turns_zero_is_loud_error() {
    let llm = ScriptedLlm::new(vec![]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let err = run_agent(
        &config_for_turns(0, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect_err("run_agent must reject max_turns=0");

    assert!(matches!(err, AgentError::InvalidConfig(_)));
}

#[tokio::test]
async fn edge_duplicate_tool_call_id_within_turn_is_loud_error() {
    let llm = ScriptedLlm::new(vec![Ok(turn_with_calls(vec![
        tool_call("tc_dup", "echo"),
        tool_call("tc_dup", "echo"),
    ]))]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let err = run_agent(
        &config_for_turns(2, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect_err("duplicate tool_call_id must be rejected");

    match err {
        AgentError::DuplicateToolCallId { tool_call_id } => {
            assert_eq!(tool_call_id, "tc_dup");
        }
        other => panic!("expected DuplicateToolCallId, got {other:?}"),
    }
}

#[tokio::test]
async fn edge_pending_user_decision_returns_waiting_for_approval() {
    let llm = ScriptedLlm::new(vec![Ok(turn_with_calls(vec![tool_call("tc_1", "ask")]))]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();

    let result = run_agent(
        &config_for_turns(3, ToolApprovalPolicy::None), // None = always Ask
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect("run_agent ok");

    assert_eq!(result.reason, AgentLoopReason::WaitingForApproval);
    assert_eq!(result.pending_tool_calls, vec!["tc_1".to_string()]);
}

#[tokio::test]
async fn edge_cancellation_aborts_run() {
    let llm = ScriptedLlm::new(vec![Ok(final_answer("never"))]);
    let executor = EchoExecutor;
    let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
    let reducer = PassthroughContextReducer;
    let compactor = PassthroughCompactionEngine;
    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = run_agent(
        &config_for_turns(1, ToolApprovalPolicy::AcceptAll),
        &run_ctx(),
        vec![AgentMessage::user("kick")],
        &[],
        &executor,
        &hooks,
        &llm,
        &reducer,
        &compactor,
        &cancel,
    )
    .await
    .expect_err("cancelled run must error out");

    assert!(matches!(err, AgentError::Cancelled));
}
