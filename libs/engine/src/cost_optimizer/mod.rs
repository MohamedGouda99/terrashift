// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cost Optimizer agent — bounded trade-off-reasoning loop that
//! refines a migration plan to fit a cost-reduction policy.
//!
//! Pattern: terrashift_plan.md §5 (one of 3 agents permitted by Article I).
//! Source: Terrashift addition; consumes S9's `terrashift_agent_core::run_agent`.
//!
//! Constitution:
//! - Article I (Cost Optimizer is one of 3 named agents allowed without RFC).
//! - Article IV (loud terminal states — `OptimizationOutcome` exhaustive).
//! - Article XII rule 4 (max_iterations + max_turns hard caps on token spend).
//!
//! ## Loop shape (mirrors Recovery's two-budget design)
//!
//! 1. Cost the current plan via the `CostService` (Stage 2: stub or
//!    Infracost adapter when S11b lands).
//! 2. If `current_cost <= target_cost`, exit `Success`.
//! 3. Otherwise, build an optimizer prompt naming the current cost,
//!    the target, and the candidate optimizations (instance-size
//!    changes, storage-type swaps, architecture migrations); call
//!    `run_agent` for one bounded turn cycle.
//! 4. The kernel dispatches LLM-proposed cost-saving tool calls
//!    through the approval gate; `CostFixApplier` mutates the plan.
//! 5. Outer loop iterates: re-cost, repeat.
//! 6. Bail when target met / `max_iterations` exhausted / agent gives up.
//!
//! ## Why a separate `CostService` trait
//!
//! The Stage-2-structural-shell shipped here uses `StubCostService` (a
//! deterministic in-memory implementation) so tests are hermetic. The
//! production-path `InfracostCostService` (calls https://api.infracost.io
//! with the operator's API key from `~/.terrashift/profile.toml`) lands
//! in S11b once we have a test API key and have negotiated rate limits
//! with infracost.io. The trait surface is stable across both impls so
//! flipping providers is a single registration change.

use crate::mapper::{AttributeValue, MappingPlan};
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

// ---------------------------------------------------------------------
// Cost service contract
// ---------------------------------------------------------------------

/// Per-resource cost estimate. Stage 2 narrowing: just `monthly_usd`
/// (sum-of-resources). Stage 5+ widens with `per_region_breakdown`,
/// `committed_use_savings`, `data_egress`, etc.
#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimate {
    pub monthly_usd: f64,
    pub per_resource_usd: std::collections::BTreeMap<String, f64>,
}

#[derive(Debug, Error)]
pub enum CostServiceError {
    #[error("cost service network error: {0}")]
    Network(String),

    #[error("cost service auth error: {0}")]
    Auth(String),

    #[error("cost service unsupported resource type: {0}")]
    UnsupportedType(String),
}

#[async_trait]
pub trait CostService: Send + Sync {
    async fn estimate(&self, plan: &MappingPlan) -> Result<CostEstimate, CostServiceError>;
}

// ---------------------------------------------------------------------
// Optimizer config / outcome
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    pub max_iterations: usize,
    /// Stop when `current_cost <= target_monthly_usd`.
    pub target_monthly_usd: f64,
    pub agent_loop: AgentLoopConfig,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            max_iterations: 4,
            target_monthly_usd: 0.0,
            agent_loop: AgentLoopConfig {
                max_turns: 4,
                approval_policy: ToolApprovalPolicy::AcceptAll,
                ..AgentLoopConfig::default()
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum OptimizerError {
    #[error("cost service failed: {0}")]
    Cost(#[from] CostServiceError),

    #[error("agent kernel error: {0}")]
    AgentKernel(#[from] AgentError),

    #[error("invalid optimizer config: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum OptimizationOutcome {
    /// Target cost reached (or already below at start).
    Success {
        iterations: usize,
        fixes_applied: usize,
        starting_cost_usd: f64,
        final_cost_usd: f64,
    },
    /// max_iterations exhausted; final cost above target.
    MaxIterationsReached {
        iterations: usize,
        starting_cost_usd: f64,
        final_cost_usd: f64,
    },
    /// LLM emitted final-answer turn while still above target.
    AgentGaveUp {
        iterations: usize,
        starting_cost_usd: f64,
        final_cost_usd: f64,
    },
    WaitingForApproval {
        iterations: usize,
        pending_tool_call_ids: Vec<String>,
    },
}

// ---------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn run_optimizer(
    config: &OptimizerConfig,
    plan: MappingPlan,
    cost_service: &dyn CostService,
    llm: &dyn AgentLlmClient,
    reducer: &dyn ContextReducer,
    compactor: &dyn CompactionEngine,
    hooks: &[Box<dyn AgentHook>],
    cancel: &CancellationToken,
) -> Result<(MappingPlan, OptimizationOutcome), OptimizerError> {
    if config.max_iterations == 0 {
        return Err(OptimizerError::InvalidConfig(
            "max_iterations must be > 0".to_string(),
        ));
    }
    if config.target_monthly_usd < 0.0 {
        return Err(OptimizerError::InvalidConfig(
            "target_monthly_usd must be non-negative".to_string(),
        ));
    }

    let plan_state = Arc::new(Mutex::new(plan));
    let fix_count = Arc::new(AtomicUsize::new(0));
    let executor = CostFixApplier {
        plan: Arc::clone(&plan_state),
        fix_count: Arc::clone(&fix_count),
    };
    let tools = optimizer_tool_defs();

    let starting_snapshot = plan_state
        .lock()
        .map_err(|_| OptimizerError::InvalidConfig("plan lock poisoned".to_string()))?
        .clone();
    let starting_cost = cost_service.estimate(&starting_snapshot).await?;

    if starting_cost.monthly_usd <= config.target_monthly_usd {
        return Ok((
            starting_snapshot,
            OptimizationOutcome::Success {
                iterations: 0,
                fixes_applied: 0,
                starting_cost_usd: starting_cost.monthly_usd,
                final_cost_usd: starting_cost.monthly_usd,
            },
        ));
    }

    for iteration in 1..=config.max_iterations {
        let snapshot = plan_state
            .lock()
            .map_err(|_| OptimizerError::InvalidConfig("plan lock poisoned".to_string()))?
            .clone();
        let cost = cost_service.estimate(&snapshot).await?;

        if cost.monthly_usd <= config.target_monthly_usd {
            return Ok((
                snapshot,
                OptimizationOutcome::Success {
                    iterations: iteration - 1,
                    fixes_applied: fix_count.load(Ordering::SeqCst),
                    starting_cost_usd: starting_cost.monthly_usd,
                    final_cost_usd: cost.monthly_usd,
                },
            ));
        }

        let prompt = build_optimizer_prompt(&snapshot, &cost, config.target_monthly_usd);
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
                    .map_err(|_| OptimizerError::InvalidConfig("plan lock poisoned".to_string()))?
                    .clone();
                return Ok((
                    plan,
                    OptimizationOutcome::WaitingForApproval {
                        iterations: iteration,
                        pending_tool_call_ids: pending_tool_calls,
                    },
                ));
            }
            AgentLoopResult {
                reason: AgentLoopReason::FinalAnswerReached,
                ..
            } if fixes_this_iter == 0 => {
                let plan = plan_state
                    .lock()
                    .map_err(|_| OptimizerError::InvalidConfig("plan lock poisoned".to_string()))?
                    .clone();
                let final_cost = cost_service.estimate(&plan).await?;
                return Ok((
                    plan,
                    OptimizationOutcome::AgentGaveUp {
                        iterations: iteration,
                        starting_cost_usd: starting_cost.monthly_usd,
                        final_cost_usd: final_cost.monthly_usd,
                    },
                ));
            }
            _ => {}
        }
    }

    let plan = plan_state
        .lock()
        .map_err(|_| OptimizerError::InvalidConfig("plan lock poisoned".to_string()))?
        .clone();
    let final_cost = cost_service.estimate(&plan).await?;
    Ok((
        plan,
        OptimizationOutcome::MaxIterationsReached {
            iterations: config.max_iterations,
            starting_cost_usd: starting_cost.monthly_usd,
            final_cost_usd: final_cost.monthly_usd,
        },
    ))
}

// ---------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------

fn optimizer_tool_defs() -> Vec<AgentToolDef> {
    vec![
        AgentToolDef {
            name: "set_attribute".to_string(),
            description: "Set or replace an attribute value on a target resource. \
                Use to right-size instances (e.g., instance_type='t3.large' → 't3.medium'), \
                migrate to cheaper storage (e.g., type='gp2' → 'gp3'), or change architecture \
                (e.g., instance_type='m5.large' → 'm6g.large' for Graviton)."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["target_addr", "attribute", "value"],
                "properties": {
                    "target_addr": { "type": "string" },
                    "attribute":   { "type": "string" },
                    "value":       {}
                }
            }),
        },
        AgentToolDef {
            name: "remove_resource".to_string(),
            description: "Remove a planned resource entirely. Last-resort optimization \
                (e.g., redundant standby instance). Operator review strongly recommended."
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
// Tool executor — same shape as Recovery's FixApplier, narrowed to
// optimization-specific tools.
// ---------------------------------------------------------------------

struct CostFixApplier {
    plan: Arc<Mutex<MappingPlan>>,
    fix_count: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolExecutor for CostFixApplier {
    async fn execute_tool_call(
        &self,
        _ctx: &AgentRunContext,
        tool_call: &ProposedToolCall,
        _cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        let mut plan = self
            .plan
            .lock()
            .map_err(|_| AgentError::ToolExecution("plan lock poisoned".to_string()))?;

        let outcome = match tool_call.name.as_str() {
            "set_attribute" => apply_set_attribute(&mut plan, &tool_call.arguments),
            "remove_resource" => apply_remove_resource(&mut plan, &tool_call.arguments),
            other => Ok(ToolExecutionResult::Completed {
                result: format!("unknown optimizer tool: {other}"),
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
    let attr_value = json_to_attribute_value(raw_value)?;

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
            result: format!("set_attribute: target_addr '{target_addr}' not found"),
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

fn json_to_attribute_value(value: &Value) -> Result<AttributeValue, AgentError> {
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
                out.push(json_to_attribute_value(item)?);
            }
            Ok(AttributeValue::List(out))
        }
        Value::Object(map) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in map {
                out.insert(k.clone(), json_to_attribute_value(v)?);
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

fn build_optimizer_prompt(plan: &MappingPlan, cost: &CostEstimate, target_usd: f64) -> String {
    use std::fmt::Write as _;

    let mut p = String::new();
    let _ = writeln!(
        p,
        "You are the Terrashift Cost Optimizer agent. The current plan estimates ${:.2}/month, \
        but the operator's target is ${:.2}/month. Your job: propose targeted optimizations \
        via the available tools to bring the cost under target.",
        cost.monthly_usd, target_usd
    );
    let _ = writeln!(p);
    let _ = writeln!(
        p,
        "Source: {} | Target: {} | Resources: {}\n",
        plan.source_provider,
        plan.target_provider,
        plan.resources.len()
    );

    let _ = writeln!(p, "## Per-resource monthly cost (USD)\n");
    let mut entries: Vec<_> = cost.per_resource_usd.iter().collect();
    entries.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (addr, usd) in entries.iter().take(10) {
        let _ = writeln!(p, "- {addr}: ${usd:.2}");
    }

    let _ = writeln!(p, "\n## Available tools\n");
    let _ = writeln!(
        p,
        "- `set_attribute(target_addr, attribute, value)` — change instance type, storage class, etc."
    );
    let _ = writeln!(
        p,
        "- `remove_resource(target_addr)` — drop redundant resource (last-resort)"
    );

    let _ = writeln!(p, "\n## Optimization patterns to consider\n");
    let _ = writeln!(
        p,
        "- gp2 → gp3 storage (typically ~20% cheaper at same IOPS)"
    );
    let _ = writeln!(
        p,
        "- x86 (m5/c5) → Graviton (m6g/c6g) (typically ~10-20% cheaper)"
    );
    let _ = writeln!(p, "- Right-size oversized instances (instance_type)");
    let _ = writeln!(
        p,
        "- Drop redundant standby resources where workload allows"
    );

    let _ = writeln!(p, "\n## Constraints\n");
    let _ = writeln!(p, "- Only use the named tools.");
    let _ = writeln!(p, "- Preserve correctness — don't propose changes that break the architecture (e.g., changing a primary instance to a much smaller one without considering load).");
    let _ = writeln!(
        p,
        "- Each optimization is auto-approved by the operator's policy."
    );

    p
}

// ---------------------------------------------------------------------
// Stub cost service (for tests)
// ---------------------------------------------------------------------

/// Hermetic test stub. Maps each resource's `target_addr` to a fixed
/// monthly cost; sums them. Production replaces with `InfracostCostService`
/// in S11b.
pub struct StubCostService {
    pub fixed_per_addr: std::collections::HashMap<String, f64>,
    pub default_per_resource_usd: f64,
}

impl StubCostService {
    pub fn new() -> Self {
        Self {
            fixed_per_addr: std::collections::HashMap::new(),
            default_per_resource_usd: 100.0,
        }
    }

    pub fn with_cost(mut self, target_addr: impl Into<String>, usd: f64) -> Self {
        self.fixed_per_addr.insert(target_addr.into(), usd);
        self
    }

    pub fn with_default(mut self, usd: f64) -> Self {
        self.default_per_resource_usd = usd;
        self
    }
}

impl Default for StubCostService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CostService for StubCostService {
    async fn estimate(&self, plan: &MappingPlan) -> Result<CostEstimate, CostServiceError> {
        let mut per_resource = std::collections::BTreeMap::new();
        let mut total = 0.0_f64;
        for r in &plan.resources {
            let usd = self
                .fixed_per_addr
                .get(&r.target_addr)
                .copied()
                .unwrap_or(self.default_per_resource_usd);
            per_resource.insert(r.target_addr.clone(), usd);
            total += usd;
        }
        Ok(CostEstimate {
            monthly_usd: total,
            per_resource_usd: per_resource,
        })
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::mapper::MappedResource;
    use std::collections::BTreeMap;

    fn one_resource_plan(target_type: &str) -> MappingPlan {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "instance_type".to_string(),
            AttributeValue::String("t3.large".to_string()),
        );
        MappingPlan {
            run_id: Uuid::new_v4(),
            source_provider: "google".to_string(),
            target_provider: "aws".to_string(),
            resources: vec![MappedResource {
                source_addr: "src.x".to_string(),
                target_addr: format!("{target_type}.main"),
                target_type: target_type.to_string(),
                target_name: "main".to_string(),
                attributes: attrs,
                dependencies: Vec::new(),
            }],
        }
    }

    #[tokio::test]
    async fn stub_cost_service_returns_default_per_resource() {
        let plan = one_resource_plan("aws_instance");
        let service = StubCostService::new().with_default(50.0);
        let estimate = service.estimate(&plan).await.expect("ok");
        assert_eq!(estimate.monthly_usd, 50.0);
        assert_eq!(estimate.per_resource_usd.len(), 1);
    }

    #[tokio::test]
    async fn stub_cost_service_uses_fixed_addr_override() {
        let plan = one_resource_plan("aws_instance");
        let service = StubCostService::new().with_cost("aws_instance.main", 250.0);
        let estimate = service.estimate(&plan).await.expect("ok");
        assert_eq!(estimate.monthly_usd, 250.0);
    }

    #[test]
    fn optimizer_prompt_lists_top_costly_resources() {
        let plan = one_resource_plan("aws_instance");
        let mut per = BTreeMap::new();
        per.insert("aws_instance.main".to_string(), 200.0);
        let cost = CostEstimate {
            monthly_usd: 200.0,
            per_resource_usd: per,
        };
        let prompt = build_optimizer_prompt(&plan, &cost, 100.0);
        assert!(prompt.contains("$200.00"));
        assert!(prompt.contains("$100.00"));
        assert!(prompt.contains("aws_instance.main"));
        assert!(prompt.contains("set_attribute"));
        assert!(prompt.contains("Graviton") || prompt.contains("gp3"));
    }

    #[tokio::test]
    async fn cost_fix_applier_set_attribute_updates_plan() {
        let plan = one_resource_plan("aws_instance");
        let plan_state = Arc::new(Mutex::new(plan));
        let fix_count = Arc::new(AtomicUsize::new(0));
        let applier = CostFixApplier {
            plan: Arc::clone(&plan_state),
            fix_count,
        };

        let call = ProposedToolCall {
            id: "tc_1".to_string(),
            name: "set_attribute".to_string(),
            arguments: json!({
                "target_addr": "aws_instance.main",
                "attribute": "instance_type",
                "value": "t3.medium"
            }),
            metadata: None,
        };
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
            .expect("ok");
        assert!(matches!(
            result,
            ToolExecutionResult::Completed {
                is_error: false,
                ..
            }
        ));

        let plan = plan_state.lock().unwrap();
        let attr = plan.resources[0].attributes.get("instance_type").unwrap();
        assert_eq!(*attr, AttributeValue::String("t3.medium".to_string()));
    }
}
