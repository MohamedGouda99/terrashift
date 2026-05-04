// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Recovery agent — bounded ReAct loop that asks the LLM to fix
//! Validator-blocked migrations.
//!
//! Pattern: terrashift_plan.md §5 (one of 3 allowed agents per Article I).
//! Source: Terrashift addition; uses S9's `terrashift_agent_core::run_agent`
//! kernel. No direct the reference counterpart — the reference's agent loop is generic;
//! Terrashift specializes it to "fix HCL emit errors" here.
//!
//! Constitution:
//! - Article I (Recovery is one of 3 named agents allowed by the constitution
//!   without further RFC: Recovery / Cost Optimizer / Cutover).
//! - Article III (only fires when Validator already failed — never bypasses).
//! - Article IV (loud terminal states — `RecoveryOutcome` is exhaustive).
//! - Article XII rule 2 (cache-stable: same plan + same errors must produce
//!   the same fix proposal — relies on the LLM's seed parameter when it lands
//!   in S5+; Stage 2 stub LLM is deterministic by construction).
//!
//! ## Loop shape
//!
//! 1. Re-validate the current plan.
//! 2. If `report.passed`, exit `Success`.
//! 3. Otherwise, build a recovery prompt naming each error, list the
//!    available fix tools (`set_attribute`, `change_target_type`,
//!    `remove_resource`), and call `run_agent` for one turn-bounded
//!    sub-loop.
//! 4. The kernel dispatches the LLM-proposed fix tool calls through the
//!    approval gate; `FixApplier` mutates the plan in-place.
//! 5. After the run, the outer loop iterates: validate again, repeat.
//! 6. Bail when:
//!    - Validator passes (`Success`)
//!    - LLM emitted no tool calls in a turn while errors remain
//!      (`AgentGaveUp` — model couldn't propose a fix)
//!    - `max_iterations` exhausted (`MaxIterationsReached`)
//!
//! ## Why an outer loop instead of just trusting `max_turns`
//!
//! `run_agent`'s `max_turns` bounds *one* LLM-proposed-tools cycle. Recovery
//! wants the validator to re-run between cycles so the LLM can see whether
//! its earlier proposed fixes worked. If we let the LLM make 10 fix proposals
//! in one go without re-validating, we lose the feedback loop that makes
//! ReAct effective. Two budgets, two layers of safety.

use crate::mapper::{AttributeValue, MappingPlan};
use crate::validator::{ValidationError, ValidationReport, Validator, ValidatorError};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use terrashift_agent_core::agent::{run_agent, AgentLlmClient, AgentMessage, AgentToolDef};
use terrashift_agent_core::{
    AgentError, AgentHook, AgentLoopConfig, AgentLoopReason, AgentLoopResult, AgentRunContext,
    CompactionEngine, ContextReducer, ProposedToolCall, ToolApprovalPolicy, ToolExecutionResult,
    ToolExecutor,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Top-level config. Stage 2 defaults: 5 outer iterations, kernel
/// `max_turns = 4` so each iteration can issue several related fixes
/// before a re-validate cycle.
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    pub max_iterations: usize,
    pub agent_loop: AgentLoopConfig,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_iterations: 5,
            agent_loop: AgentLoopConfig {
                max_turns: 4,
                approval_policy: ToolApprovalPolicy::AcceptAll,
                ..AgentLoopConfig::default()
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("validator failed during recovery: {0}")]
    ValidatorFailure(#[from] ValidatorError),

    #[error("agent kernel error during recovery: {0}")]
    AgentKernel(#[from] AgentError),

    #[error("invalid recovery config: {0}")]
    InvalidConfig(String),
}

/// What the Recovery loop returned. Exhaustive — no silent stalls
/// (Article IV).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// Validator now passes. `iterations` is how many outer cycles ran;
    /// `fixes_applied` is the cumulative tool-call count.
    Success {
        iterations: usize,
        fixes_applied: usize,
    },
    /// `max_iterations` exhausted while errors remained. Returned plan
    /// is the best-effort partial state — caller decides to discard or
    /// hand to operator for manual fix.
    MaxIterationsReached {
        iterations: usize,
        unresolved_errors: Vec<ValidationError>,
    },
    /// The LLM produced a final-answer turn (no tool calls) while
    /// errors still remained — model self-reported "I can't fix this".
    AgentGaveUp {
        iterations: usize,
        unresolved_errors: Vec<ValidationError>,
    },
    /// The kernel returned `WaitingForApproval` — operator interaction
    /// required before recovery can continue. Stage 2 narrowing: the
    /// caller wires this back through the TUI; S13 detached mode fully
    /// closes this path.
    WaitingForApproval {
        iterations: usize,
        pending_tool_call_ids: Vec<String>,
    },
}

/// Run the Recovery agent loop.
///
/// Mutates `plan` through the per-iteration kernel cycle; returns the
/// final state alongside a terminal `RecoveryOutcome`.
#[allow(clippy::too_many_arguments)]
pub async fn run_recovery(
    config: &RecoveryConfig,
    plan: MappingPlan,
    validator: &Validator,
    target_version: &str,
    llm: &dyn AgentLlmClient,
    reducer: &dyn ContextReducer,
    compactor: &dyn CompactionEngine,
    hooks: &[Box<dyn AgentHook>],
    cancel: &CancellationToken,
) -> Result<(MappingPlan, RecoveryOutcome), RecoveryError> {
    if config.max_iterations == 0 {
        return Err(RecoveryError::InvalidConfig(
            "max_iterations must be > 0".to_string(),
        ));
    }

    let plan_state = Arc::new(Mutex::new(plan));
    let fix_count = Arc::new(AtomicUsize::new(0));
    let executor = FixApplier {
        plan: Arc::clone(&plan_state),
        fix_count: Arc::clone(&fix_count),
    };
    let tools = recovery_tool_defs();

    for iteration in 1..=config.max_iterations {
        // 1. Re-validate the current plan.
        let snapshot = plan_state
            .lock()
            .map_err(|_| RecoveryError::InvalidConfig("plan lock poisoned".to_string()))?
            .clone();
        let report = validator.validate(&snapshot, target_version).await?;

        if report.passed {
            return Ok((
                snapshot,
                RecoveryOutcome::Success {
                    iterations: iteration - 1,
                    fixes_applied: fix_count.load(Ordering::SeqCst),
                },
            ));
        }

        // 2. Build prompt + run the kernel for one bounded cycle.
        let prompt = build_recovery_prompt(&report, &snapshot);
        let ctx = AgentRunContext {
            run_id: snapshot.run_id,
            session_id: Uuid::new_v4(),
        };

        let fixes_before = fix_count.load(Ordering::SeqCst);
        let kernel_result = run_agent(
            &config.agent_loop,
            &ctx,
            vec![AgentMessage::user(prompt)],
            &tools,
            &executor,
            hooks,
            llm,
            reducer,
            compactor,
            cancel,
        )
        .await?;
        let fixes_this_iter = fix_count
            .load(Ordering::SeqCst)
            .saturating_sub(fixes_before);

        match kernel_result {
            AgentLoopResult {
                reason: AgentLoopReason::WaitingForApproval,
                pending_tool_calls,
                ..
            } => {
                let plan = plan_state
                    .lock()
                    .map_err(|_| RecoveryError::InvalidConfig("plan lock poisoned".to_string()))?
                    .clone();
                return Ok((
                    plan,
                    RecoveryOutcome::WaitingForApproval {
                        iterations: iteration,
                        pending_tool_call_ids: pending_tool_calls,
                    },
                ));
            }
            AgentLoopResult {
                reason: AgentLoopReason::FinalAnswerReached,
                ..
            } if fixes_this_iter == 0 => {
                // LLM produced final text without proposing fixes — model
                // gave up. Record the unresolved errors and bail.
                let plan = plan_state
                    .lock()
                    .map_err(|_| RecoveryError::InvalidConfig("plan lock poisoned".to_string()))?
                    .clone();
                let final_report = validator.validate(&plan, target_version).await?;
                return Ok((
                    plan,
                    RecoveryOutcome::AgentGaveUp {
                        iterations: iteration,
                        unresolved_errors: final_report.errors,
                    },
                ));
            }
            // Otherwise: kernel ran, fixes applied (or not), continue
            // to the next outer iteration which will re-validate.
            _ => {}
        }
    }

    // Exhausted max_iterations.
    let plan = plan_state
        .lock()
        .map_err(|_| RecoveryError::InvalidConfig("plan lock poisoned".to_string()))?
        .clone();
    let final_report = validator.validate(&plan, target_version).await?;

    Ok((
        plan,
        RecoveryOutcome::MaxIterationsReached {
            iterations: config.max_iterations,
            unresolved_errors: final_report.errors,
        },
    ))
}

// ---------------------------------------------------------------------
// Tool definitions sent to the LLM
// ---------------------------------------------------------------------

fn recovery_tool_defs() -> Vec<AgentToolDef> {
    vec![
        AgentToolDef {
            name: "set_attribute".to_string(),
            description: "Set or replace an attribute value on a target resource. \
                Use to fix UnknownAttribute (rename misspelled attr) and \
                MissingRequiredAttribute (add a missing required value)."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["target_addr", "attribute", "value"],
                "properties": {
                    "target_addr": { "type": "string", "description": "e.g., 'azurerm_virtual_network.main'" },
                    "attribute":   { "type": "string", "description": "Attribute name in the target schema" },
                    "value":       { "description": "Attribute value (string / number / bool / list / map)" }
                }
            }),
        },
        AgentToolDef {
            name: "change_target_type".to_string(),
            description: "Replace the target resource type for a planned resource. \
                Use to fix UnknownResourceType (Mapper hallucinated a type)."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["target_addr", "new_type"],
                "properties": {
                    "target_addr": { "type": "string" },
                    "new_type":    { "type": "string", "description": "Correct target_type from the target schema" }
                }
            }),
        },
        AgentToolDef {
            name: "remove_resource".to_string(),
            description: "Remove a planned resource entirely. Last-resort fix for \
                hallucinations that have no valid target equivalent. Operator \
                review recommended."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["target_addr"],
                "properties": {
                    "target_addr": { "type": "string" }
                }
            }),
        },
    ]
}

// ---------------------------------------------------------------------
// Tool executor — applies fixes to the live MappingPlan
// ---------------------------------------------------------------------

struct FixApplier {
    plan: Arc<Mutex<MappingPlan>>,
    fix_count: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolExecutor for FixApplier {
    async fn execute_tool_call(
        &self,
        _ctx: &AgentRunContext,
        tool_call: &ProposedToolCall,
        _cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        let mut plan = self
            .plan
            .lock()
            .map_err(|_| AgentError::ToolExecution("Recovery: plan lock poisoned".to_string()))?;

        let outcome = match tool_call.name.as_str() {
            "set_attribute" => apply_set_attribute(&mut plan, &tool_call.arguments),
            "change_target_type" => apply_change_target_type(&mut plan, &tool_call.arguments),
            "remove_resource" => apply_remove_resource(&mut plan, &tool_call.arguments),
            other => Ok(ToolExecutionResult::Completed {
                result: format!("unknown recovery tool: {other} — no plan mutation occurred"),
                is_error: true,
            }),
        };

        if let Ok(ToolExecutionResult::Completed {
            is_error: false, ..
        }) = outcome
        {
            self.fix_count.fetch_add(1, Ordering::SeqCst);
        }

        outcome
    }
}

fn apply_set_attribute(
    plan: &mut MappingPlan,
    args: &Value,
) -> Result<ToolExecutionResult, AgentError> {
    let target_addr = string_arg(args, "target_addr")?;
    let attribute = string_arg(args, "attribute")?;
    let raw_value = args
        .get("value")
        .ok_or_else(|| AgentError::ToolExecution("set_attribute: missing 'value'".to_string()))?;

    let attr_value = json_value_to_attribute_value(raw_value)?;

    if let Some(res) = plan
        .resources
        .iter_mut()
        .find(|r| r.target_addr == target_addr)
    {
        res.attributes.insert(attribute.clone(), attr_value);
        Ok(ToolExecutionResult::Completed {
            result: format!("set_attribute: {target_addr}.{attribute} updated"),
            is_error: false,
        })
    } else {
        Ok(ToolExecutionResult::Completed {
            result: format!("set_attribute: target_addr '{target_addr}' not found in plan"),
            is_error: true,
        })
    }
}

fn apply_change_target_type(
    plan: &mut MappingPlan,
    args: &Value,
) -> Result<ToolExecutionResult, AgentError> {
    let target_addr = string_arg(args, "target_addr")?;
    let new_type = string_arg(args, "new_type")?;

    if let Some(res) = plan
        .resources
        .iter_mut()
        .find(|r| r.target_addr == target_addr)
    {
        let old_type = res.target_type.clone();
        res.target_type = new_type.clone();
        let new_addr = format!("{}.{}", new_type, res.target_name);
        res.target_addr = new_addr.clone();
        Ok(ToolExecutionResult::Completed {
            result: format!(
                "change_target_type: {target_addr} ({old_type} → {new_type}); new addr {new_addr}"
            ),
            is_error: false,
        })
    } else {
        Ok(ToolExecutionResult::Completed {
            result: format!("change_target_type: target_addr '{target_addr}' not found"),
            is_error: true,
        })
    }
}

fn apply_remove_resource(
    plan: &mut MappingPlan,
    args: &Value,
) -> Result<ToolExecutionResult, AgentError> {
    let target_addr = string_arg(args, "target_addr")?;

    let before = plan.resources.len();
    plan.resources.retain(|r| r.target_addr != target_addr);
    if plan.resources.len() < before {
        Ok(ToolExecutionResult::Completed {
            result: format!("remove_resource: removed {target_addr}"),
            is_error: false,
        })
    } else {
        Ok(ToolExecutionResult::Completed {
            result: format!("remove_resource: target_addr '{target_addr}' not found"),
            is_error: true,
        })
    }
}

fn string_arg(args: &Value, key: &str) -> Result<String, AgentError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            AgentError::ToolExecution(format!("missing or non-string argument: '{key}'"))
        })
}

fn json_value_to_attribute_value(value: &Value) -> Result<AttributeValue, AgentError> {
    match value {
        Value::String(s) => Ok(AttributeValue::String(s.clone())),
        Value::Number(n) => n
            .as_f64()
            .map(AttributeValue::Number)
            .ok_or_else(|| AgentError::ToolExecution(format!("number out of range: {n}"))),
        Value::Bool(b) => Ok(AttributeValue::Bool(*b)),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_value_to_attribute_value(item)?);
            }
            Ok(AttributeValue::List(out))
        }
        Value::Object(map) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in map {
                out.insert(k.clone(), json_value_to_attribute_value(v)?);
            }
            Ok(AttributeValue::Map(out))
        }
        Value::Null => Err(AgentError::ToolExecution(
            "null is not a valid attribute value".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------
// Prompt building
// ---------------------------------------------------------------------

fn build_recovery_prompt(report: &ValidationReport, plan: &MappingPlan) -> String {
    use std::fmt::Write as _;

    let mut prompt = String::new();
    let _ = writeln!(
        prompt,
        "You are the Terrashift Recovery agent. The Validator rejected the migration plan \
        with {} error(s). Your job: propose targeted fixes via the available tools \
        until the Validator passes.",
        report.errors.len()
    );
    let _ = writeln!(prompt);
    let _ = writeln!(
        prompt,
        "Source provider: {}\nTarget provider: {}\nResources in plan: {}\n",
        plan.source_provider,
        plan.target_provider,
        plan.resources.len()
    );

    let _ = writeln!(prompt, "## Validator errors\n");
    for (i, err) in report.errors.iter().enumerate() {
        let _ = writeln!(prompt, "{}. {err}", i + 1);
    }

    let _ = writeln!(prompt, "\n## Available tools\n");
    let _ = writeln!(
        prompt,
        "- `set_attribute(target_addr, attribute, value)` — fix UnknownAttribute / MissingRequiredAttribute"
    );
    let _ = writeln!(
        prompt,
        "- `change_target_type(target_addr, new_type)` — fix UnknownResourceType"
    );
    let _ = writeln!(
        prompt,
        "- `remove_resource(target_addr)` — last-resort for unrecoverable hallucinations"
    );

    let _ = writeln!(prompt, "\n## Constraints\n");
    let _ = writeln!(
        prompt,
        "- Only use the named tools. Do not invent new tools."
    );
    let _ = writeln!(
        prompt,
        "- If you cannot fix an error, say so explicitly in your final answer; do not silently skip."
    );
    let _ = writeln!(
        prompt,
        "- Each fix is auto-approved by the operator policy in this run."
    );

    prompt
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::mapper::{AttributeValue, MappedResource};
    use serde_json::json;
    use std::collections::BTreeMap;

    // --- Stubs reused across tests --------------------------------

    fn tool_call(id: &str, name: &str, args: Value) -> ProposedToolCall {
        ProposedToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args,
            metadata: None,
        }
    }

    fn one_resource_plan(target_type: &str, attr_key: &str, attr_val: &str) -> MappingPlan {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            attr_key.to_string(),
            AttributeValue::String(attr_val.to_string()),
        );
        MappingPlan {
            run_id: Uuid::new_v4(),
            source_provider: "google".to_string(),
            target_provider: "aws".to_string(),
            resources: vec![MappedResource {
                source_addr: "google_compute_network.main".to_string(),
                target_addr: format!("{target_type}.main"),
                target_type: target_type.to_string(),
                target_name: "main".to_string(),
                attributes: attrs,
                dependencies: Vec::new(),
            }],
        }
    }

    // --- FixApplier unit tests ------------------------------------

    fn make_fix_applier(plan: MappingPlan) -> (FixApplier, Arc<Mutex<MappingPlan>>) {
        let plan_state = Arc::new(Mutex::new(plan));
        let fix_count = Arc::new(AtomicUsize::new(0));
        (
            FixApplier {
                plan: Arc::clone(&plan_state),
                fix_count,
            },
            plan_state,
        )
    }

    #[tokio::test]
    async fn fix_applier_set_attribute_updates_plan() {
        let plan = one_resource_plan("aws_vpc", "cidr_blocks", "10.0.0.0/16");
        let (applier, plan_state) = make_fix_applier(plan);

        let call = tool_call(
            "tc_1",
            "set_attribute",
            json!({"target_addr": "aws_vpc.main", "attribute": "cidr_block", "value": "10.0.0.0/16"}),
        );
        let result = applier
            .execute_tool_call(
                &AgentRunContext {
                    run_id: Uuid::new_v4(),
                    session_id: Uuid::new_v4(),
                },
                &call,
                &CancellationToken::new(),
            )
            .await
            .expect("execute ok");

        assert!(matches!(
            result,
            ToolExecutionResult::Completed {
                is_error: false,
                ..
            }
        ));
        let plan = plan_state.lock().unwrap();
        assert!(plan.resources[0].attributes.contains_key("cidr_block"));
    }

    #[tokio::test]
    async fn fix_applier_change_target_type_updates_addr() {
        let plan = one_resource_plan("aws_buckets", "name", "x");
        let (applier, plan_state) = make_fix_applier(plan);

        let call = tool_call(
            "tc_1",
            "change_target_type",
            json!({"target_addr": "aws_buckets.main", "new_type": "aws_s3_bucket"}),
        );
        applier
            .execute_tool_call(
                &AgentRunContext {
                    run_id: Uuid::new_v4(),
                    session_id: Uuid::new_v4(),
                },
                &call,
                &CancellationToken::new(),
            )
            .await
            .expect("execute ok");

        let plan = plan_state.lock().unwrap();
        assert_eq!(plan.resources[0].target_type, "aws_s3_bucket");
        assert_eq!(plan.resources[0].target_addr, "aws_s3_bucket.main");
    }

    #[tokio::test]
    async fn fix_applier_remove_resource_drops_from_plan() {
        let plan = one_resource_plan("aws_invalid", "x", "y");
        let (applier, plan_state) = make_fix_applier(plan);

        let call = tool_call(
            "tc_1",
            "remove_resource",
            json!({"target_addr": "aws_invalid.main"}),
        );
        applier
            .execute_tool_call(
                &AgentRunContext {
                    run_id: Uuid::new_v4(),
                    session_id: Uuid::new_v4(),
                },
                &call,
                &CancellationToken::new(),
            )
            .await
            .expect("execute ok");

        let plan = plan_state.lock().unwrap();
        assert!(plan.resources.is_empty());
    }

    #[tokio::test]
    async fn fix_applier_unknown_target_addr_is_error() {
        let plan = one_resource_plan("aws_vpc", "cidr_block", "x");
        let (applier, _) = make_fix_applier(plan);

        let call = tool_call(
            "tc_1",
            "set_attribute",
            json!({"target_addr": "missing.thing", "attribute": "x", "value": "y"}),
        );
        let result = applier
            .execute_tool_call(
                &AgentRunContext {
                    run_id: Uuid::new_v4(),
                    session_id: Uuid::new_v4(),
                },
                &call,
                &CancellationToken::new(),
            )
            .await
            .expect("execute ok");

        match result {
            ToolExecutionResult::Completed { is_error, result } => {
                assert!(is_error, "missing target should be an error");
                assert!(result.contains("not found"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fix_applier_unknown_tool_name_is_error_not_panic() {
        let plan = one_resource_plan("aws_vpc", "cidr_block", "x");
        let (applier, _) = make_fix_applier(plan);

        let call = tool_call("tc_1", "definitely_not_a_tool", json!({}));
        let result = applier
            .execute_tool_call(
                &AgentRunContext {
                    run_id: Uuid::new_v4(),
                    session_id: Uuid::new_v4(),
                },
                &call,
                &CancellationToken::new(),
            )
            .await
            .expect("execute ok");
        match result {
            ToolExecutionResult::Completed { is_error, result } => {
                assert!(is_error);
                assert!(result.contains("unknown recovery tool"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    // --- Prompt building unit tests --------------------------------

    #[test]
    fn recovery_prompt_lists_each_error() {
        let plan = one_resource_plan("aws_buckets", "x", "y");
        let report = ValidationReport::new(
            vec![
                ValidationError::UnknownResourceType {
                    addr: "aws_buckets.main".to_string(),
                    target_type: "aws_buckets".to_string(),
                },
                ValidationError::MissingRequiredAttribute {
                    addr: "aws_s3_bucket.x".to_string(),
                    attr: "bucket".to_string(),
                    target_type: "aws_s3_bucket".to_string(),
                },
            ],
            Vec::new(),
        );
        let prompt = build_recovery_prompt(&report, &plan);
        assert!(prompt.contains("aws_buckets"));
        assert!(prompt.contains("MissingRequiredAttribute") || prompt.contains("missing required"));
        assert!(prompt.contains("set_attribute"));
        assert!(prompt.contains("change_target_type"));
    }

    // --- Outer-loop integration tests (with stubbed validator) -----
    //
    // The full integration path with a real `Validator` requires a
    // KnowledgeService instance which is heavy to set up here. The
    // unit tests above cover the FixApplier's mutation correctness;
    // S10b will add a full integration test once libs/ai's
    // AgentLlmClient adapter ships.
}
