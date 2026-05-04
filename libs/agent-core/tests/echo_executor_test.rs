// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Integration test for `ToolExecutor` + `ToolRegistry` using a trivial
//! `EchoExecutor` that returns its input as output.
//!
//! Covers the four cases from `specs/002-tool-trait-and-executor/spec.md`:
//!   1. Happy path — EchoExecutor returns Completed{result, is_error: false}
//!   2. Cancellation — pre-cancelled token causes Cancelled result
//!   3. Registry dispatch — ToolRegistry routes to the right executor by name
//!   4. Registry unknown — unknown name returns AgentError::ToolExecution

use async_trait::async_trait;
use serde_json::json;
use terrashift_agent_core::{
    AgentError, AgentRunContext, ProposedToolCall, ToolExecutionResult, ToolExecutor, ToolRegistry,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Trivial executor — echoes back the JSON `arguments` as a string.
struct EchoExecutor;

#[async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute_tool_call(
        &self,
        _run: &AgentRunContext,
        tool_call: &ProposedToolCall,
        cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        if cancel.is_cancelled() {
            return Ok(ToolExecutionResult::Cancelled);
        }
        Ok(ToolExecutionResult::Completed {
            result: tool_call.arguments.to_string(),
            is_error: false,
        })
    }
}

fn run_ctx() -> AgentRunContext {
    AgentRunContext {
        run_id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
    }
}

fn echo_call() -> ProposedToolCall {
    ProposedToolCall {
        id: "call-1".to_string(),
        name: "echo".to_string(),
        arguments: json!({"hello": "world"}),
        metadata: None,
    }
}

#[tokio::test]
async fn echo_happy_path() {
    let executor = EchoExecutor;
    let ctx = run_ctx();
    let call = echo_call();
    let cancel = CancellationToken::new();

    let result = executor
        .execute_tool_call(&ctx, &call, &cancel)
        .await
        .unwrap_or_else(|e| panic!("expected Ok, got Err: {e}"));

    match result {
        ToolExecutionResult::Completed { result, is_error } => {
            assert!(!is_error, "expected is_error=false");
            assert!(result.contains("hello"), "expected echo of arguments");
            assert!(result.contains("world"), "expected echo of arguments");
        }
        ToolExecutionResult::Cancelled => panic!("expected Completed, got Cancelled"),
    }
}

#[tokio::test]
async fn echo_honors_cancellation() {
    let executor = EchoExecutor;
    let ctx = run_ctx();
    let call = echo_call();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let result = executor
        .execute_tool_call(&ctx, &call, &cancel)
        .await
        .unwrap_or_else(|e| panic!("expected Ok, got Err: {e}"));

    assert_eq!(result, ToolExecutionResult::Cancelled);
}

#[tokio::test]
async fn registry_dispatches_by_name() {
    let mut registry = ToolRegistry::new();
    registry.register("echo", EchoExecutor);

    let ctx = run_ctx();
    let call = echo_call();
    let cancel = CancellationToken::new();

    let result = registry
        .execute_tool_call(&ctx, &call, &cancel)
        .await
        .unwrap_or_else(|e| panic!("expected Ok, got Err: {e}"));

    match result {
        ToolExecutionResult::Completed {
            result,
            is_error: false,
        } => {
            assert!(result.contains("hello"));
        }
        other => panic!("expected Completed{{is_error:false}}, got {other:?}"),
    }

    let names = registry.registered_names();
    assert_eq!(names, vec!["echo".to_string()]);
}

#[tokio::test]
async fn registry_unknown_tool_errors_loud() {
    let mut registry = ToolRegistry::new();
    registry.register("echo", EchoExecutor);

    let ctx = run_ctx();
    let call = ProposedToolCall {
        id: "call-2".to_string(),
        name: "nonexistent".to_string(),
        arguments: json!({}),
        metadata: None,
    };
    let cancel = CancellationToken::new();

    let err = registry
        .execute_tool_call(&ctx, &call, &cancel)
        .await
        .err()
        .unwrap_or_else(|| panic!("expected Err, got Ok"));

    match err {
        AgentError::ToolExecution(msg) => {
            assert!(
                msg.contains("nonexistent"),
                "msg should mention the bad name"
            );
            assert!(msg.contains("echo"), "msg should list registered names");
        }
        other => panic!("expected AgentError::ToolExecution, got {other:?}"),
    }
}
