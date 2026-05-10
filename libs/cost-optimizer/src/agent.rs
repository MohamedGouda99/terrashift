// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Cost Optimizer agent — bounded ReAct loop that wraps Phase 2's
//! deterministic `CostOptimizer::analyze` and asks the LLM to populate
//! `CostReport::recommendations` via three narrow tools.
//!
//! Pattern: stakpak_arch.md §8 (canonical agent loop kernel) and §12 (LLM
//! tier router — eco tier; cost-optimizer hits cheap models because every
//! lookup is read-only and short-context).
//! Source: Terrashift addition; the canonical reference is the Recovery
//! agent in `libs/engine/src/recovery/mod.rs` — this module mirrors that
//! shape 1:1, narrowed for "collect recommendations" instead of "mutate
//! plan".
//!
//! Constitution:
//! - Article I (Cost Optimizer is the 3rd of 3 agents permitted without
//!   further RFC — Recovery, Cost Optimizer, Cutover).
//! - Article III (fires AFTER Validator+Generator — the deterministic
//!   `analyze` runs first; the agent only enriches the report, never
//!   bypasses earlier stages).
//! - Article IV (loud terminal states — `CostAgentOutcome` is exhaustive;
//!   tool parse errors surface as `is_error: true` on a Completed
//!   ToolExecutionResult, NOT propagated through `?`).
//! - Article V (no attribute leakage — the `lookup_resource_cost` and
//!   `lookup_alternative` tools only see provider+type+region; the
//!   `recommend_swap` tool may carry `current_attrs`/`proposed_attrs` but
//!   those are LLM-authored summaries, not raw secret-bearing values).
//! - Article XII rule 4 (regression-gate plumbing — Phase 5 eval will
//!   measure token cost; this phase keeps spend bounded via
//!   `delta_threshold_pct` so we only invoke the agent on resources
//!   where target>source by enough to matter).
//! - Article XIII rule 6 (recommend_swap is `Ask` in production —
//!   the `AcceptAll` default below is for hermetic Stage-2-bringup tests
//!   only; Stage 4+ flips this when the CLI gains an approval prompt).
//!
//! ## Loop shape (mirrors Recovery's two-budget design)
//!
//! 1. Run the deterministic `CostOptimizer::analyze` first to compute
//!    line items and totals.
//! 2. Filter to "candidate" line items where target_cost > source_cost
//!    by `delta_threshold_pct`.
//! 3. If no candidates, return `CostAgentOutcome::NoCandidates` with the
//!    deterministic report unchanged.
//! 4. Otherwise, build a prompt naming the candidates + tools, and call
//!    `run_agent` for one bounded turn cycle.
//! 5. The kernel dispatches LLM-proposed lookup + recommend tool calls
//!    through the approval gate; `RecommendationCollector` appends each
//!    accepted `recommend_swap` to the report.
//! 6. Outer loop iterates: re-check candidates that still exceed
//!    threshold (after the LLM may have proposed swaps for some), repeat.
//! 7. Bail when:
//!    - LLM emitted a final-answer turn (no tool calls) — `Success` if
//!      any recommendations were collected, `AgentGaveUp` otherwise.
//!    - `max_iterations` exhausted → `MaxIterationsReached`.
//!    - WaitingForApproval surfaces (Stage-2-bringup tests use AcceptAll
//!      so this should not occur; Stage 4+ wires this through the CLI).

use crate::analyze::{AnalyzeConfig, CostLookup, CostOptimizer};
use crate::errors::CostOptimizerError;
use crate::report::{CostReport, LineItem, Recommendation};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use terrashift_agent_core::agent::{run_agent, AgentLlmClient, AgentMessage, AgentToolDef};
use terrashift_agent_core::{
    AgentError, AgentHook, AgentLoopConfig, AgentLoopReason, AgentLoopResult, AgentRunContext,
    PassthroughCompactionEngine, PassthroughContextReducer, ProposedToolCall, ToolApprovalPolicy,
    ToolExecutionResult, ToolExecutor,
};
use terrashift_engine::mapper::MappingPlan;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ---------------------------------------------------------------------
// Config / outcome / errors
// ---------------------------------------------------------------------

/// Top-level config. Stage 2 defaults: 4 outer iterations, kernel
/// `max_turns = 4` so each iteration can issue several related lookups
/// before a re-check cycle.
///
/// **Article XIII rule 6 caveat:** the default `approval_policy` is
/// `AcceptAll`. Production deployments should switch to `Custom` with
/// `recommend_swap` mapped to `Ask` per Article XIII rule 6. The
/// `AcceptAll` default is for hermetic tests + Stage-2-bringup; Stage 4+
/// flips this when the CLI gains an approval prompt.
#[derive(Debug, Clone)]
pub struct CostAgentConfig {
    /// Outer-loop budget. `0` is invalid (returns `InvalidConfig`).
    pub max_iterations: usize,
    /// Inner-loop budget passed to `run_agent`. `max_turns` bounds one
    /// LLM-proposed-tools cycle; the outer loop iterates if more
    /// candidates remain.
    pub agent_loop: AgentLoopConfig,
    /// Only invoke the agent for resources where `target_cost` exceeds
    /// `source_cost` by at least this percentage. Bounds spend per
    /// Article XII rule 4 — sub-threshold deltas aren't worth a token.
    pub delta_threshold_pct: f64,
}

impl Default for CostAgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 4,
            agent_loop: AgentLoopConfig {
                max_turns: 4,
                approval_policy: ToolApprovalPolicy::AcceptAll,
                ..AgentLoopConfig::default()
            },
            // 10% — a $10/mo target vs $11/mo source (10% delta) gets the
            // agent treatment; smaller deltas fall through. Tuned during
            // Phase 5 eval.
            delta_threshold_pct: 10.0,
        }
    }
}

/// Why the cost-optimizer agent loop stopped. Exhaustive — no silent
/// stalls (Article IV).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostAgentOutcome {
    /// Agent ran and at least one `recommend_swap` was accepted, OR the
    /// LLM cleanly finished after exploring lookups (zero recommendations
    /// is fine when the LLM concluded no swap is justified).
    Success {
        iterations: usize,
        recommendations_collected: usize,
    },
    /// Deterministic analyze produced zero candidates — every resource's
    /// target cost was within `delta_threshold_pct` of source. Nothing
    /// for the agent to do; report returned unchanged.
    NoCandidates,
    /// LLM emitted a final-answer turn (no tool calls) before any
    /// recommendation was collected. Model self-reported "I can't
    /// suggest a useful swap".
    AgentGaveUp { iterations: usize },
    /// Outer-loop iteration budget exhausted while candidates remained.
    /// The collected recommendations (possibly partial) are still in
    /// the returned report.
    MaxIterationsReached {
        iterations: usize,
        recommendations_collected: usize,
    },
    /// Kernel returned `WaitingForApproval`. Stage-2-bringup tests use
    /// `AcceptAll` so this should not occur there; Stage 4+ wires this
    /// back through the TUI/CLI. Kept exhaustive per Article IV.
    WaitingForApproval {
        iterations: usize,
        pending_tool_call_ids: Vec<String>,
    },
}

/// Wrapper-level error type. Distinguishes the agent's own concerns
/// (config, kernel) from the deterministic analyzer's failures (which
/// surface as `CostOptimizerError`).
#[derive(Debug, Error)]
pub enum CostOptimizerAgentError {
    #[error("invalid agent config: {0}")]
    InvalidConfig(String),

    #[error("agent kernel error during cost optimization: {0}")]
    AgentKernel(#[from] AgentError),

    #[error("deterministic analyze failed before agent run: {0}")]
    Analyze(#[from] CostOptimizerError),
}

// ---------------------------------------------------------------------
// Agent struct
// ---------------------------------------------------------------------

/// Wraps the deterministic `CostOptimizer<L>` with an LLM-driven agent
/// that populates `CostReport::recommendations`.
///
/// Generic over `L: CostLookup` (the deterministic lookup seam — same as
/// Phase 2) and `M: AgentLlmClient` (the kernel's LLM seam).
pub struct CostOptimizerAgent<L, M>
where
    L: CostLookup,
    M: AgentLlmClient,
{
    optimizer: CostOptimizer<L>,
    llm: M,
    config: CostAgentConfig,
}

impl<L, M> CostOptimizerAgent<L, M>
where
    L: CostLookup,
    M: AgentLlmClient,
{
    /// Construct with default `CostAgentConfig`.
    pub fn new(optimizer: CostOptimizer<L>, llm: M) -> Self {
        Self {
            optimizer,
            llm,
            config: CostAgentConfig::default(),
        }
    }

    /// Override the default config. Builder-style for ergonomics.
    pub fn with_config(mut self, config: CostAgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Run analyze() first; for each line item where target > source by
    /// at least `delta_threshold_pct`, invoke the agent which can call:
    ///   - `lookup_resource_cost` (read-only, Approve)
    ///   - `lookup_alternative` (read-only, Approve)
    ///   - `recommend_swap` (Ask in production, Approve in tests —
    ///     mutates `report.recommendations` via the collector)
    ///
    /// Returns the populated `CostReport` + the terminal outcome.
    pub async fn run(
        &self,
        plan: &MappingPlan,
        analyze_config: &AnalyzeConfig,
    ) -> Result<(CostReport, CostAgentOutcome), CostOptimizerAgentError> {
        if self.config.max_iterations == 0 {
            return Err(CostOptimizerAgentError::InvalidConfig(
                "max_iterations must be > 0".to_string(),
            ));
        }
        if !self.config.delta_threshold_pct.is_finite() || self.config.delta_threshold_pct < 0.0 {
            return Err(CostOptimizerAgentError::InvalidConfig(
                "delta_threshold_pct must be a finite non-negative number".to_string(),
            ));
        }

        // Phase 2 deterministic core runs first (Article III: agent never
        // bypasses earlier stages).
        let report = self.optimizer.analyze(plan, analyze_config).await?;

        // Bail early when nothing crosses the threshold — saves all
        // tokens (Article XII rule 4).
        if !has_candidates(&report.line_items, self.config.delta_threshold_pct) {
            return Ok((report, CostAgentOutcome::NoCandidates));
        }

        // Set up shared state for the kernel-driven inner loop.
        let report_state = Arc::new(Mutex::new(report));
        let recommendation_count = Arc::new(AtomicUsize::new(0));

        let executor = CostOptimizerToolExecutor {
            report: Arc::clone(&report_state),
            recommendation_count: Arc::clone(&recommendation_count),
        };
        let tools = cost_agent_tool_defs();

        // Stateless kernel adapters — Stage 2 narrowing per agent_core
        // module map.
        let reducer = PassthroughContextReducer;
        let compactor = PassthroughCompactionEngine;
        let hooks: Vec<Box<dyn AgentHook>> = Vec::new();
        let cancel = CancellationToken::new();

        for iteration in 1..=self.config.max_iterations {
            // Re-fetch the snapshot so we can reflect any
            // recommendations the LLM has added so far. Compute the
            // prompt + early-exit check inside a tight scope so the
            // MutexGuard is not held across the upcoming `.await`
            // (clippy: await_holding_lock; std::sync::Mutex is not
            // Send-friendly across .await points).
            let prompt = {
                let snapshot = report_state.lock().map_err(|_| {
                    CostOptimizerAgentError::InvalidConfig("report lock poisoned".to_string())
                })?;
                let candidates =
                    collect_candidates(&snapshot.line_items, self.config.delta_threshold_pct);
                if candidates.is_empty() {
                    // All previously-flagged candidates have been addressed
                    // (or the LLM somehow drained them earlier). Treat as
                    // Success and return.
                    let final_count = recommendation_count.load(Ordering::SeqCst);
                    let final_report = snapshot.clone();
                    drop(snapshot);
                    return Ok((
                        final_report,
                        CostAgentOutcome::Success {
                            iterations: iteration - 1,
                            recommendations_collected: final_count,
                        },
                    ));
                }
                build_cost_agent_prompt(&candidates, plan)
            };

            let ctx = AgentRunContext {
                run_id: plan.run_id,
                session_id: Uuid::new_v4(),
            };

            let recs_before = recommendation_count.load(Ordering::SeqCst);
            let kernel_result = run_agent(
                &self.config.agent_loop,
                &ctx,
                vec![AgentMessage::user(prompt)],
                &tools,
                &executor,
                &hooks,
                &self.llm,
                &reducer,
                &compactor,
                &cancel,
            )
            .await?;
            let recs_this_iter = recommendation_count
                .load(Ordering::SeqCst)
                .saturating_sub(recs_before);

            match kernel_result {
                AgentLoopResult {
                    reason: AgentLoopReason::WaitingForApproval,
                    pending_tool_calls,
                    ..
                } => {
                    let final_report = report_state
                        .lock()
                        .map_err(|_| {
                            CostOptimizerAgentError::InvalidConfig(
                                "report lock poisoned".to_string(),
                            )
                        })?
                        .clone();
                    return Ok((
                        final_report,
                        CostAgentOutcome::WaitingForApproval {
                            iterations: iteration,
                            pending_tool_call_ids: pending_tool_calls,
                        },
                    ));
                }
                AgentLoopResult {
                    reason: AgentLoopReason::FinalAnswerReached,
                    ..
                } => {
                    let total_recs = recommendation_count.load(Ordering::SeqCst);
                    let final_report = report_state
                        .lock()
                        .map_err(|_| {
                            CostOptimizerAgentError::InvalidConfig(
                                "report lock poisoned".to_string(),
                            )
                        })?
                        .clone();
                    if recs_this_iter == 0 && total_recs == 0 {
                        // No swap proposed in this turn AND none across
                        // the whole run — model gave up.
                        return Ok((
                            final_report,
                            CostAgentOutcome::AgentGaveUp {
                                iterations: iteration,
                            },
                        ));
                    }
                    return Ok((
                        final_report,
                        CostAgentOutcome::Success {
                            iterations: iteration,
                            recommendations_collected: total_recs,
                        },
                    ));
                }
                // MaxTurnsReached / LlmRetryExhausted-as-loop-state: continue
                // outer loop (re-evaluate candidates next iteration).
                _ => {}
            }
        }

        // Outer-loop iterations exhausted. Return whatever was collected.
        let final_count = recommendation_count.load(Ordering::SeqCst);
        let final_report = report_state
            .lock()
            .map_err(|_| {
                CostOptimizerAgentError::InvalidConfig("report lock poisoned".to_string())
            })?
            .clone();
        Ok((
            final_report,
            CostAgentOutcome::MaxIterationsReached {
                iterations: self.config.max_iterations,
                recommendations_collected: final_count,
            },
        ))
    }
}

// ---------------------------------------------------------------------
// Tool definitions sent to the LLM
// ---------------------------------------------------------------------

fn cost_agent_tool_defs() -> Vec<AgentToolDef> {
    vec![
        AgentToolDef {
            name: "lookup_resource_cost".to_string(),
            description: "Read-only: look up the on-demand monthly cost of a single \
                resource type in a region. Use to refine your understanding \
                of a candidate before proposing an alternative."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["provider", "resource_type", "region"],
                "properties": {
                    "provider":      { "type": "string", "description": "e.g., 'aws', 'azurerm', 'google'" },
                    "resource_type": { "type": "string", "description": "e.g., 'aws_instance', 'azurerm_linux_virtual_machine'" },
                    "region":        { "type": "string", "description": "e.g., 'us-east-1', 'eastus'" }
                }
            }),
        },
        AgentToolDef {
            name: "lookup_alternative".to_string(),
            description: "Read-only: ask for cheaper-tier alternatives for a given \
                resource type. Returns up to 3 candidates with their \
                monthly cost. Use to find concrete swap targets."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["provider", "resource_type", "region"],
                "properties": {
                    "provider":            { "type": "string" },
                    "resource_type":       { "type": "string" },
                    "region":              { "type": "string" },
                    "target_savings_pct":  { "type": "number", "description": "Optional. Filter to candidates >= this % cheaper." }
                }
            }),
        },
        AgentToolDef {
            name: "recommend_swap".to_string(),
            description: "Mutating: record a recommendation to swap one or more \
                attributes on a resource for cost savings. Use after \
                lookup_alternative gives you a concrete cheaper candidate. \
                In production this requires operator approval (Article XIII rule 6); \
                in Stage-2-bringup tests it is auto-accepted."
                .to_string(),
            schema: json!({
                "type": "object",
                "required": ["resource_addr", "current_attrs", "proposed_attrs", "savings_usd_per_month", "tradeoff"],
                "properties": {
                    "resource_addr":           { "type": "string", "description": "target_addr from the plan, e.g., 'azurerm_linux_virtual_machine.web_a'" },
                    "current_attrs":           { "type": "object", "description": "Current attribute snapshot, e.g., {\"size\": \"Standard_D4s_v5\"}" },
                    "proposed_attrs":          { "type": "object", "description": "Proposed attribute snapshot, e.g., {\"size\": \"Standard_B4ms\"}" },
                    "savings_usd_per_month":   { "type": "number", "description": "Estimated monthly USD savings (positive number)" },
                    "tradeoff":                { "type": "string", "description": "One-sentence summary of what the operator gives up, e.g., 'burst CPU vs sustained'" }
                }
            }),
        },
    ]
}

// ---------------------------------------------------------------------
// Tool executor — the only mutator is `recommend_swap`, which appends
// to `RecommendationCollector` (the report's recommendations vec under
// a Mutex). The two lookup tools are read-only and delegate to the
// agent's deterministic CostLookup seam (or, for Phase-3 stub purposes,
// to a hardcoded candidate list — see `lookup_alternative`).
// ---------------------------------------------------------------------

/// Tool executor wrapping a shared, mutex-guarded `CostReport` plus a
/// counter for tests / outcome bookkeeping.
struct CostOptimizerToolExecutor {
    report: Arc<Mutex<CostReport>>,
    recommendation_count: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolExecutor for CostOptimizerToolExecutor {
    async fn execute_tool_call(
        &self,
        _ctx: &AgentRunContext,
        tool_call: &ProposedToolCall,
        _cancel: &CancellationToken,
    ) -> Result<ToolExecutionResult, AgentError> {
        // Article IV at the kernel boundary: tool-level errors land as
        // `is_error: true` on a Completed result, NEVER propagated
        // through `?` (that would abort the entire run).
        let outcome = match tool_call.name.as_str() {
            "lookup_resource_cost" => execute_lookup_resource_cost(&tool_call.arguments),
            "lookup_alternative" => execute_lookup_alternative(&tool_call.arguments),
            "recommend_swap" => self.execute_recommend_swap(&tool_call.arguments),
            other => Ok(ToolExecutionResult::Completed {
                result: format!("unknown cost-optimizer tool: {other} — no state mutation"),
                is_error: true,
            }),
        };

        outcome
    }
}

impl CostOptimizerToolExecutor {
    fn execute_recommend_swap(&self, args: &Value) -> Result<ToolExecutionResult, AgentError> {
        let resource_addr = match string_arg(args, "resource_addr") {
            Ok(v) => v,
            Err(msg) => {
                return Ok(ToolExecutionResult::Completed {
                    result: msg,
                    is_error: true,
                });
            }
        };
        let current_attrs = args.get("current_attrs").cloned().unwrap_or(Value::Null);
        let proposed_attrs = args.get("proposed_attrs").cloned().unwrap_or(Value::Null);
        let savings = match args
            .get("savings_usd_per_month")
            .and_then(Value::as_f64)
            .ok_or_else(|| {
                "recommend_swap: missing or non-numeric 'savings_usd_per_month'".to_string()
            }) {
            Ok(v) => v,
            Err(msg) => {
                return Ok(ToolExecutionResult::Completed {
                    result: msg,
                    is_error: true,
                });
            }
        };
        let tradeoff = match string_arg(args, "tradeoff") {
            Ok(v) => v,
            Err(msg) => {
                return Ok(ToolExecutionResult::Completed {
                    result: msg,
                    is_error: true,
                });
            }
        };

        let recommendation = Recommendation {
            resource_addr: resource_addr.clone(),
            current_attrs,
            proposed_attrs,
            savings_usd_per_month: savings,
            tradeoff,
        };

        let mut report = self.report.lock().map_err(|_| {
            AgentError::ToolExecution("CostOptimizer: report lock poisoned".to_string())
        })?;
        report.recommendations.push(recommendation);
        drop(report);

        self.recommendation_count.fetch_add(1, Ordering::SeqCst);

        Ok(ToolExecutionResult::Completed {
            result: serde_json::to_string(&json!({
                "accepted": true,
                "stored": resource_addr,
            }))
            .unwrap_or_else(|_| "{\"accepted\":true}".to_string()),
            is_error: false,
        })
    }
}

fn execute_lookup_resource_cost(args: &Value) -> Result<ToolExecutionResult, AgentError> {
    // Phase 3 narrowing: the tool acknowledges the request shape and
    // returns a stable, low-fidelity stub. The real lookup goes through
    // the deterministic `CostLookup` seam (Phase 2) when wired by the
    // agent's analyze() prepass — this read-only tool exists so the LLM
    // can re-query in case it wants a single value without the broader
    // analyze report. Production wires this through `CostLookup::lookup`
    // when Stage 4 ships the live CLI flow.
    let provider = match string_arg(args, "provider") {
        Ok(v) => v,
        Err(msg) => {
            return Ok(ToolExecutionResult::Completed {
                result: msg,
                is_error: true,
            });
        }
    };
    let resource_type = match string_arg(args, "resource_type") {
        Ok(v) => v,
        Err(msg) => {
            return Ok(ToolExecutionResult::Completed {
                result: msg,
                is_error: true,
            });
        }
    };
    let region = match string_arg(args, "region") {
        Ok(v) => v,
        Err(msg) => {
            return Ok(ToolExecutionResult::Completed {
                result: msg,
                is_error: true,
            });
        }
    };

    let stub_cost = stub_cost_for(&provider, &resource_type);
    Ok(ToolExecutionResult::Completed {
        result: serde_json::to_string(&json!({
            "usd_per_month": stub_cost,
            "source_link": format!("phase3-stub://{provider}/{resource_type}/{region}"),
        }))
        .unwrap_or_else(|_| "{}".to_string()),
        is_error: false,
    })
}

fn execute_lookup_alternative(args: &Value) -> Result<ToolExecutionResult, AgentError> {
    // TODO(stage-4): Replace this hard-coded candidate list with a real
    // catalog-backed lookup. This is the ONLY TODO permitted in Phase 3
    // per the implementation plan (every other gap surfaces as an
    // exhaustive enum variant). The Stage-4+ impl will read from
    // `libs/knowledge` once the alternatives catalog is curated.
    let provider = match string_arg(args, "provider") {
        Ok(v) => v,
        Err(msg) => {
            return Ok(ToolExecutionResult::Completed {
                result: msg,
                is_error: true,
            });
        }
    };
    let resource_type = match string_arg(args, "resource_type") {
        Ok(v) => v,
        Err(msg) => {
            return Ok(ToolExecutionResult::Completed {
                result: msg,
                is_error: true,
            });
        }
    };

    let target_savings_pct = args
        .get("target_savings_pct")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    let candidates = stub_alternatives_for(&provider, &resource_type, target_savings_pct);
    Ok(ToolExecutionResult::Completed {
        result: serde_json::to_string(&json!({
            "candidates": candidates,
        }))
        .unwrap_or_else(|_| "{\"candidates\":[]}".to_string()),
        is_error: false,
    })
}

fn string_arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing or non-string argument: '{key}'"))
}

/// Phase 3 stub cost for `lookup_resource_cost`. Returns deterministic
/// per-(provider,type) values so tests are reproducible. Stage 4 swaps
/// this for a real `CostLookup` call.
fn stub_cost_for(provider: &str, resource_type: &str) -> f64 {
    match (provider, resource_type) {
        ("aws", "aws_instance") => 130.0,
        ("aws", "aws_s3_bucket") => 2.3,
        ("aws", "aws_ebs_volume") => 12.0,
        ("azurerm", "azurerm_linux_virtual_machine") => 175.0,
        ("azurerm", "azurerm_storage_account") => 3.1,
        ("google", "google_compute_instance") => 140.0,
        _ => 50.0,
    }
}

/// Phase 3 stub alternatives table. Stage 4 swaps for a curated catalog
/// from `libs/knowledge` (per the TODO above).
///
/// Returns concrete cheaper candidates as JSON-serializable shape:
/// `[ { "alternative": "Standard_B4ms", "usd_per_month": 88.5 } ]`.
fn stub_alternatives_for(provider: &str, resource_type: &str, min_savings_pct: f64) -> Vec<Value> {
    let raw = match (provider, resource_type) {
        ("azurerm", "azurerm_linux_virtual_machine") => vec![
            ("Standard_B4ms", 88.50),
            ("Standard_D2s_v5", 92.30),
            ("Standard_F4s_v2", 110.00),
        ],
        ("aws", "aws_instance") => vec![
            ("t3.medium", 30.20),
            ("t3.large", 60.40),
            ("m6g.large", 56.00), // Graviton
        ],
        ("google", "google_compute_instance") => vec![
            ("e2-standard-2", 48.30),
            ("n2-standard-2", 71.40),
            ("t2d-standard-2", 52.10),
        ],
        _ => Vec::new(),
    };
    let baseline = stub_cost_for(provider, resource_type);
    raw.into_iter()
        .filter(|(_, alt)| {
            if min_savings_pct <= 0.0 {
                return true;
            }
            let savings_pct = (baseline - alt) / baseline * 100.0;
            savings_pct >= min_savings_pct
        })
        .map(|(alt, usd)| {
            json!({
                "alternative": alt,
                "usd_per_month": usd,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------
// Candidate selection + prompt
// ---------------------------------------------------------------------

/// Select line items where target cost exceeds source cost by at least
/// `threshold_pct`. Items with missing source/target cost are skipped
/// (already surfaced via `report.skipped` in Phase 2).
fn collect_candidates(items: &[LineItem], threshold_pct: f64) -> Vec<&LineItem> {
    items
        .iter()
        .filter(|item| match item.delta_pct() {
            Some(pct) => pct >= threshold_pct,
            None => false,
        })
        .collect()
}

fn has_candidates(items: &[LineItem], threshold_pct: f64) -> bool {
    !collect_candidates(items, threshold_pct).is_empty()
}

fn build_cost_agent_prompt(candidates: &[&LineItem], plan: &MappingPlan) -> String {
    use std::fmt::Write as _;

    let mut prompt = String::new();
    let _ = writeln!(
        prompt,
        "You are the Terrashift Cost Optimizer agent. The deterministic analyzer \
        flagged {} candidate resource(s) where the target cost exceeds the source \
        cost by enough to be worth investigating. Your job: investigate via the \
        read-only lookup tools, then emit `recommend_swap` for each justifiable \
        substitution.",
        candidates.len()
    );
    let _ = writeln!(prompt);
    let _ = writeln!(
        prompt,
        "Source provider: {}\nTarget provider: {}\n",
        plan.source_provider, plan.target_provider
    );

    let _ = writeln!(prompt, "## Candidates (target_addr — Δ$/mo, Δ%)\n");
    for (i, item) in candidates.iter().enumerate() {
        let delta_usd = item
            .delta_usd_per_month()
            .map(|v| format!("+${v:.2}"))
            .unwrap_or_else(|| "?".to_string());
        let delta_pct = item
            .delta_pct()
            .map(|v| format!("{v:+.1}%"))
            .unwrap_or_else(|| "?".to_string());
        let _ = writeln!(
            prompt,
            "{}. {} ({} → {}) — {} {}",
            i + 1,
            item.target_addr,
            item.source_type,
            item.target_type,
            delta_usd,
            delta_pct,
        );
    }

    let _ = writeln!(prompt, "\n## Available tools\n");
    let _ = writeln!(
        prompt,
        "- `lookup_resource_cost(provider, resource_type, region)` — read-only single price"
    );
    let _ = writeln!(
        prompt,
        "- `lookup_alternative(provider, resource_type, region, target_savings_pct?)` — read-only candidate list"
    );
    let _ = writeln!(
        prompt,
        "- `recommend_swap(resource_addr, current_attrs, proposed_attrs, savings_usd_per_month, tradeoff)` — record a swap suggestion"
    );

    let _ = writeln!(prompt, "\n## Constraints\n");
    let _ = writeln!(prompt, "- Only use the named tools.");
    let _ = writeln!(
        prompt,
        "- Each recommendation must include an explicit `tradeoff` sentence (Article V — \
        operators see this; do not gloss over what they're giving up)."
    );
    let _ = writeln!(
        prompt,
        "- If you cannot justify any swap for a candidate, emit a final answer \
        explaining why; do not silently skip."
    );

    prompt
}

// ---------------------------------------------------------------------
// Tests (unit; the integration tests live in tests/agent_test.rs)
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    fn item(target_addr: &str, src: Option<f64>, tgt: Option<f64>) -> LineItem {
        LineItem {
            source_addr: format!("src.{target_addr}"),
            target_addr: target_addr.to_string(),
            source_type: "aws_instance".to_string(),
            target_type: "azurerm_linux_virtual_machine".to_string(),
            region: "eastus".to_string(),
            source_usd_per_month: src,
            target_usd_per_month: tgt,
        }
    }

    #[test]
    fn collect_candidates_filters_by_pct() {
        let items = vec![
            item("a", Some(10.0), Some(11.0)), // 10% — above threshold
            item("b", Some(10.0), Some(10.5)), // 5% — below
            item("c", Some(10.0), Some(20.0)), // 100%
            item("d", None, None),             // missing — skipped
        ];
        let candidates = collect_candidates(&items, 10.0);
        let addrs: Vec<&str> = candidates.iter().map(|i| i.target_addr.as_str()).collect();
        assert!(addrs.contains(&"a"));
        assert!(addrs.contains(&"c"));
        assert!(!addrs.contains(&"b"));
        assert!(!addrs.contains(&"d"));
    }

    #[test]
    fn stub_alternatives_returns_known_azurerm_candidates() {
        let alts = stub_alternatives_for("azurerm", "azurerm_linux_virtual_machine", 0.0);
        assert_eq!(alts.len(), 3);
        // First candidate is Standard_B4ms at $88.50.
        let first = &alts[0];
        assert_eq!(
            first.get("alternative").and_then(Value::as_str),
            Some("Standard_B4ms")
        );
    }

    #[test]
    fn stub_alternatives_filters_by_min_savings() {
        // Baseline: 175.0; B4ms is 88.5 → 49.4% savings
        let alts = stub_alternatives_for("azurerm", "azurerm_linux_virtual_machine", 60.0);
        // None of the three are >=60% cheaper than 175.
        assert_eq!(alts.len(), 0);
    }

    #[test]
    fn stub_alternatives_unknown_pair_returns_empty() {
        let alts = stub_alternatives_for("nope", "nope_type", 0.0);
        assert_eq!(alts.len(), 0);
    }

    #[test]
    fn cost_agent_prompt_lists_each_candidate() {
        let plan = MappingPlan {
            run_id: Uuid::new_v4(),
            source_provider: "aws".to_string(),
            target_provider: "azurerm".to_string(),
            resources: Vec::new(),
        };
        let items = [item("a.x", Some(10.0), Some(20.0))];
        let candidates: Vec<&LineItem> = items.iter().collect();
        let prompt = build_cost_agent_prompt(&candidates, &plan);
        assert!(prompt.contains("a.x"));
        assert!(prompt.contains("recommend_swap"));
        assert!(prompt.contains("lookup_alternative"));
    }
}
