// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! HashMap-backed multi-tool registry — convenience layer over ToolExecutor.
//!
//! **Terrashift addition** — not in the reference. Convenience layer over
//! `ToolExecutor` for multi-tool registration. See
//! `specs/002-tool-trait-and-executor/clarify.md` Q3 for the rationale:
//! every crate that wants to register multiple tools (`libs/engine` for
//! Scanner+Mapper+Validator+Generator+Executor+Verifier) would otherwise
//! re-invent this dispatch HashMap. ToolRegistry IS just that helper —
//! it implements `ToolExecutor` by looking up `tool_call.name`.
//!
//! the reference's pattern is to have one `ToolExecutor` impl per crate that
//! pattern-matches on `tool_call.name` directly. That works for stable tool
//! sets but is awkward for the per-pipeline-component growth path Terrashift
//! follows (P-04 adds Scanner, P-05 adds Mapper, etc., each in its own crate
//! eventually). Registry decouples registration from dispatch.
//!
//! Constitution: Article XI (this is a deviation from the reference — registered
//! here as an additive convenience, not a replacement for ToolExecutor).

use crate::{
    error::AgentError,
    tools::{ToolExecutionResult, ToolExecutor},
    types::{AgentRunContext, ProposedToolCall},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Multi-tool dispatcher: maps `tool_call.name` to a registered `ToolExecutor`.
///
/// Each registered executor is responsible for handling its own subset of
/// tool names — typically a single tool, but a registered executor can
/// internally fan out to many names if it wants. The registry simply does
/// first-match lookup.
///
/// Lookup is O(1) by name. Cloning is cheap (Arc<dyn ToolExecutor>).
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an executor under a tool name. Replaces any prior binding
    /// for that name.
    pub fn register<E>(&mut self, name: impl Into<String>, executor: E) -> &mut Self
    where
        E: ToolExecutor + 'static,
    {
        self.tools.insert(name.into(), Arc::new(executor));
        self
    }

    /// Register a pre-shared executor (useful when one executor handles
    /// multiple tool names — register the same Arc under each name).
    pub fn register_arc(
        &mut self,
        name: impl Into<String>,
        executor: Arc<dyn ToolExecutor>,
    ) -> &mut Self {
        self.tools.insert(name.into(), executor);
        self
    }

    /// Names of all registered tools, sorted alphabetically (stable for tests).
    pub fn registered_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }
}

#[async_trait]
impl ToolExecutor for ToolRegistry {
    async fn execute_tool_call(
        &self,
        run: &AgentRunContext,
        tool_call: &ProposedToolCall,
        cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        match self.tools.get(&tool_call.name) {
            Some(executor) => executor.execute_tool_call(run, tool_call, cancel).await,
            None => Err(AgentError::ToolExecution(format!(
                "no tool registered with name '{}' (registered: {:?})",
                tool_call.name,
                self.registered_names()
            ))),
        }
    }
}
