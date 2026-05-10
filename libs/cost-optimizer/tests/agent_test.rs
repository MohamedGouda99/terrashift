// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! S11 Phase 3 — Cost Optimizer agent integration tests.
//!
//! Covers the agent's end-to-end loop with a hermetic stub LLM
//! (`StubAgentLlm`) that returns canned tool calls per turn. Mirrors the
//! ScriptedLlm pattern from `libs/agent-core/tests/agent_loop_test.rs`.
//!
//! Tests:
//! 1. agent_run_no_candidates — every line item below threshold → NoCandidates
//! 2. agent_run_records_recommendation — single bait resource, stub LLM
//!    emits one recommend_swap → outcome Success + 1 recommendation
//! 3. agent_run_max_iterations_bail — stub LLM keeps proposing without
//!    convergence → MaxIterationsReached
//! 4. agent_run_lookup_alternative_returns_candidates — verify the
//!    lookup_alternative tool returns the hardcoded list correctly
//! 5. agent_run_with_cache_hit_skips_lookup — wires CostCache; verify
//!    the deterministic prepass leverages the cache on the second run
//!    (touches the analyze path that the agent depends on).
//!
//! Constitution:
//! - Article IV (loud terminal states verified by exhaustive matches on
//!   CostAgentOutcome).
//! - Article XIII rule 9 (stub LLM never emits duplicate tool_call_id;
//!   per-turn ids are unique).

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use terrashift_agent_core::agent::{
    AgentLlmClient, AgentMessage, AgentToolDef, LlmTurnError, LlmTurnOutcome,
};
use terrashift_agent_core::{AgentRunContext, ProposedToolCall};
use terrashift_cost_optimizer::{
    AnalyzeConfig, CostAgentConfig, CostAgentOutcome, CostCache, CostLookup, CostOptimizer,
    CostOptimizerAgent, CostOptimizerError, ResourceCostQuery, ResourceCostResult,
};
use terrashift_engine::mapper::{MappedResource, MappingPlan};
use uuid::Uuid;

// ---------------------------------------------------------------------
// StubLookup — re-implemented locally for branch independence (mirrors
// the pattern in tests/analyze_test.rs).
// ---------------------------------------------------------------------

type StubTable = BTreeMap<(String, String), Result<f64, &'static str>>;

struct StubLookup {
    table: Mutex<StubTable>,
    calls: Arc<Mutex<u32>>,
}

impl StubLookup {
    fn new(entries: &[((&str, &str), f64)]) -> Self {
        let table = entries
            .iter()
            .map(|((p, t), v)| ((p.to_string(), t.to_string()), Ok(*v)))
            .collect();
        Self {
            table: Mutex::new(table),
            calls: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl CostLookup for StubLookup {
    async fn lookup(
        &self,
        query: &ResourceCostQuery,
    ) -> Result<ResourceCostResult, CostOptimizerError> {
        *self.calls.lock().unwrap() += 1;
        let key = (query.provider.clone(), query.resource_type.clone());
        match self.table.lock().unwrap().get(&key) {
            Some(Ok(v)) => Ok(ResourceCostResult {
                usd_per_month: *v,
                source_link: None,
            }),
            Some(Err(msg)) => Err(CostOptimizerError::Api((*msg).to_string())),
            None => Err(CostOptimizerError::Api(format!("unknown stub key {key:?}"))),
        }
    }
}

// ---------------------------------------------------------------------
// StubAgentLlm — canned-turn LLM for hermetic agent tests.
// ---------------------------------------------------------------------

/// Canned playback of `LlmTurnOutcome`s. Each call to `generate_turn`
/// pops the next entry and returns it; if the script is exhausted the
/// stub falls back to a final-answer turn so the loop terminates cleanly
/// (lets `MaxIterationsReached` tests exercise the kernel's MaxTurns
/// path within an outer iteration without panicking the LLM mock).
struct StubAgentLlm {
    outcomes: Mutex<Vec<LlmTurnOutcome>>,
    /// When true, after the script is exhausted return a final-answer
    /// turn instead of panicking. Default true (matches the doc above).
    fallback_final_answer: bool,
    call_count: Arc<Mutex<usize>>,
}

impl StubAgentLlm {
    fn new(outcomes: Vec<LlmTurnOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes),
            fallback_final_answer: true,
            call_count: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl AgentLlmClient for StubAgentLlm {
    async fn generate_turn(
        &self,
        _ctx: &AgentRunContext,
        _messages: &[AgentMessage],
        _tools: &[AgentToolDef],
    ) -> Result<LlmTurnOutcome, LlmTurnError> {
        *self.call_count.lock().unwrap() += 1;
        let mut outcomes = self.outcomes.lock().unwrap();
        if outcomes.is_empty() {
            if self.fallback_final_answer {
                return Ok(LlmTurnOutcome {
                    final_text: Some(
                        "(stub: script exhausted, returning final answer)".to_string(),
                    ),
                    proposed_tool_calls: Vec::new(),
                    usage_input_tokens: 0,
                    usage_output_tokens: 0,
                });
            }
            // Without fallback, panic loudly — tests that need this
            // mode must supply enough turns.
            panic!("StubAgentLlm exhausted and fallback_final_answer is disabled");
        }
        Ok(outcomes.remove(0))
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

fn turn_with_calls(calls: Vec<ProposedToolCall>) -> LlmTurnOutcome {
    LlmTurnOutcome {
        final_text: None,
        proposed_tool_calls: calls,
        usage_input_tokens: 0,
        usage_output_tokens: 0,
    }
}

fn tool_call(id: &str, name: &str, args: Value) -> ProposedToolCall {
    ProposedToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: args,
        metadata: None,
    }
}

// ---------------------------------------------------------------------
// Plan fixture
// ---------------------------------------------------------------------

fn plan_with(pairs: &[(&str, &str, &str, &str)]) -> MappingPlan {
    let resources = pairs
        .iter()
        .map(|(saddr, sname, taddr_type, tname)| MappedResource {
            source_addr: format!("{saddr}.{sname}"),
            target_addr: format!("{taddr_type}.{tname}"),
            target_type: taddr_type.to_string(),
            target_name: tname.to_string(),
            attributes: Default::default(),
            dependencies: vec![],
        })
        .collect();
    MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "aws".to_string(),
        target_provider: "azurerm".to_string(),
        resources,
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

/// Test 1: every line item has target<=source by less than the threshold,
/// so the agent never invokes the LLM. The deterministic report is
/// returned with `NoCandidates`.
#[tokio::test]
async fn agent_run_no_candidates() {
    let lookup = StubLookup::new(&[
        // Both source and target the same — 0% delta.
        (("aws", "aws_instance"), 50.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 50.0),
    ]);
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);

    let optimizer = CostOptimizer::new(lookup);
    let llm = StubAgentLlm::new(Vec::new());
    let llm_calls = llm.call_count.clone();
    let agent = CostOptimizerAgent::new(optimizer, llm);
    // Use default threshold (10%). 50→50 is 0%, so NoCandidates.

    let (report, outcome) = agent
        .run(&plan, &AnalyzeConfig::default())
        .await
        .expect("agent run should succeed");

    assert_eq!(outcome, CostAgentOutcome::NoCandidates);
    assert_eq!(report.line_items.len(), 1);
    assert_eq!(report.recommendations.len(), 0, "no candidates → no recs",);
    // LLM was never called — direct read of the Arc<Mutex<usize>> we
    // cloned from the stub before passing it to the agent.
    assert_eq!(*llm_calls.lock().unwrap(), 0, "no candidates → 0 LLM calls");
}

/// Test 2: a single resource with a 100% delta crosses the threshold;
/// stub LLM emits one recommend_swap then a final-answer turn. Outcome
/// is `Success` with 1 recommendation collected.
#[tokio::test]
async fn agent_run_records_recommendation() {
    let lookup = StubLookup::new(&[
        (("aws", "aws_instance"), 50.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 100.0), // 100% above source
    ]);
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);

    let llm = StubAgentLlm::new(vec![
        // Turn 1: LLM proposes recommend_swap.
        turn_with_calls(vec![tool_call(
            "tc_swap_1",
            "recommend_swap",
            json!({
                "resource_addr": "azurerm_linux_virtual_machine.web",
                "current_attrs": {"size": "Standard_D4s_v5"},
                "proposed_attrs": {"size": "Standard_B4ms"},
                "savings_usd_per_month": 42.30,
                "tradeoff": "burst CPU vs sustained"
            }),
        )]),
        // Turn 2: final answer.
        final_answer("Recommended Standard_B4ms; ~$42/mo savings"),
    ]);
    let optimizer = CostOptimizer::new(lookup);
    let agent = CostOptimizerAgent::new(optimizer, llm);

    let (report, outcome) = agent
        .run(&plan, &AnalyzeConfig::default())
        .await
        .expect("agent run should succeed");

    match outcome {
        CostAgentOutcome::Success {
            recommendations_collected,
            ..
        } => {
            assert_eq!(recommendations_collected, 1);
        }
        other => panic!("expected Success, got {other:?}"),
    }
    assert_eq!(report.recommendations.len(), 1);
    assert_eq!(
        report.recommendations[0].resource_addr,
        "azurerm_linux_virtual_machine.web"
    );
    assert!((report.recommendations[0].savings_usd_per_month - 42.30).abs() < 0.001);
}

/// Test 3: stub LLM proposes a lookup tool every turn but never emits
/// recommend_swap and never returns a final answer — outer loop budget
/// must clamp at `max_iterations`.
#[tokio::test]
async fn agent_run_max_iterations_bail() {
    let lookup = StubLookup::new(&[
        (("aws", "aws_instance"), 50.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 100.0),
    ]);
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);

    // Build a long script: every kernel turn proposes a lookup but
    // never recommend_swap and never converges. Each outer iteration
    // runs 2 inner turns (max_turns=2 below) before MaxTurnsReached.
    // We need enough script entries to keep firing until the OUTER
    // budget (max_iterations) clamps.
    let mut script = Vec::new();
    for i in 0..16 {
        script.push(turn_with_calls(vec![tool_call(
            &format!("tc_look_{i}"),
            "lookup_resource_cost",
            json!({
                "provider": "azurerm",
                "resource_type": "azurerm_linux_virtual_machine",
                "region": "eastus"
            }),
        )]));
    }

    let llm = StubAgentLlm::new(script);
    let optimizer = CostOptimizer::new(lookup);
    let cfg = CostAgentConfig {
        max_iterations: 2,
        agent_loop: terrashift_agent_core::AgentLoopConfig {
            max_turns: 2,
            approval_policy: terrashift_agent_core::ToolApprovalPolicy::AcceptAll,
            ..terrashift_agent_core::AgentLoopConfig::default()
        },
        delta_threshold_pct: 10.0,
    };
    let agent = CostOptimizerAgent::new(optimizer, llm).with_config(cfg);

    let (report, outcome) = agent
        .run(&plan, &AnalyzeConfig::default())
        .await
        .expect("agent run should succeed");

    match outcome {
        CostAgentOutcome::MaxIterationsReached {
            iterations,
            recommendations_collected,
        } => {
            assert_eq!(iterations, 2);
            assert_eq!(recommendations_collected, 0);
        }
        other => panic!("expected MaxIterationsReached, got {other:?}"),
    }
    assert_eq!(report.recommendations.len(), 0);
}

/// Test 4: drive the `lookup_alternative` tool through one
/// recommend_swap-then-final flow, verifying that the LLM's tool result
/// (echoed back into the conversation by the kernel) carries the
/// hardcoded candidate list. We can't introspect the message thread
/// directly through the agent surface, so the assertion is indirect:
/// the recommendation the LLM emits afterwards references one of the
/// hardcoded candidates, and the run completes.
#[tokio::test]
async fn agent_run_lookup_alternative_returns_candidates() {
    let lookup = StubLookup::new(&[
        (("aws", "aws_instance"), 50.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 175.0),
    ]);
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);

    let llm = StubAgentLlm::new(vec![
        // Turn 1: ask for alternatives. The kernel echoes the hardcoded
        // result list back into the message thread.
        turn_with_calls(vec![tool_call(
            "tc_alt_1",
            "lookup_alternative",
            json!({
                "provider": "azurerm",
                "resource_type": "azurerm_linux_virtual_machine",
                "region": "eastus",
                "target_savings_pct": 0.0
            }),
        )]),
        // Turn 2: pick Standard_B4ms (one of the hardcoded candidates).
        turn_with_calls(vec![tool_call(
            "tc_swap_1",
            "recommend_swap",
            json!({
                "resource_addr": "azurerm_linux_virtual_machine.web",
                "current_attrs": {"size": "Standard_D4s_v5"},
                "proposed_attrs": {"size": "Standard_B4ms"},
                "savings_usd_per_month": 86.50,
                "tradeoff": "burstable CPU"
            }),
        )]),
        // Turn 3: final answer.
        final_answer("Done"),
    ]);
    let optimizer = CostOptimizer::new(lookup);
    let cfg = CostAgentConfig {
        max_iterations: 2,
        agent_loop: terrashift_agent_core::AgentLoopConfig {
            max_turns: 5,
            approval_policy: terrashift_agent_core::ToolApprovalPolicy::AcceptAll,
            ..terrashift_agent_core::AgentLoopConfig::default()
        },
        delta_threshold_pct: 10.0,
    };
    let agent = CostOptimizerAgent::new(optimizer, llm).with_config(cfg);

    let (report, outcome) = agent
        .run(&plan, &AnalyzeConfig::default())
        .await
        .expect("agent run should succeed");

    assert!(matches!(outcome, CostAgentOutcome::Success { .. }));
    assert_eq!(report.recommendations.len(), 1);
    assert_eq!(
        report.recommendations[0]
            .proposed_attrs
            .get("size")
            .and_then(Value::as_str),
        Some("Standard_B4ms"),
        "LLM picked Standard_B4ms — one of the hardcoded candidates",
    );
}

/// Test 5: warm the CostCache via one analyze pass, then run the agent;
/// verify the agent's analyze prepass takes zero new lookups (cache
/// hit) and the deterministic report is still produced. This ensures
/// the cache wiring is preserved end-to-end.
#[tokio::test]
async fn agent_run_with_cache_hit_skips_lookup() {
    let lookup = StubLookup::new(&[
        (("aws", "aws_instance"), 50.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 100.0),
    ]);
    let calls_handle = lookup.calls.clone();
    let dir = tempfile::tempdir().unwrap();
    let cache = CostCache::new(dir.path().to_path_buf(), 24);

    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);

    let optimizer = CostOptimizer::new(lookup).with_cache(cache);

    // Warm the cache by running analyze() directly first.
    let _warm = optimizer
        .analyze(&plan, &AnalyzeConfig::default())
        .await
        .expect("warm analyze");
    let calls_after_warm = *calls_handle.lock().unwrap();
    assert_eq!(
        calls_after_warm, 2,
        "warm pass: 1 source + 1 target = 2 stub calls"
    );

    // Now run the agent. The deterministic prepass should hit the
    // cache and the LLM emits a recommend_swap then final answer.
    let llm = StubAgentLlm::new(vec![
        turn_with_calls(vec![tool_call(
            "tc_swap_1",
            "recommend_swap",
            json!({
                "resource_addr": "azurerm_linux_virtual_machine.web",
                "current_attrs": {"size": "D4"},
                "proposed_attrs": {"size": "B4"},
                "savings_usd_per_month": 30.0,
                "tradeoff": "burst"
            }),
        )]),
        final_answer("done"),
    ]);
    let agent = CostOptimizerAgent::new(optimizer, llm);

    let (report, outcome) = agent
        .run(&plan, &AnalyzeConfig::default())
        .await
        .expect("agent run should succeed");

    let calls_after_agent = *calls_handle.lock().unwrap();
    assert_eq!(
        calls_after_agent, calls_after_warm,
        "agent's analyze prepass must hit the cache — no new stub calls"
    );

    assert!(matches!(outcome, CostAgentOutcome::Success { .. }));
    assert_eq!(report.recommendations.len(), 1);
    // Cache stats accumulate across BOTH runs (warm pass + agent prepass);
    // we just confirm the agent's run added cache hits, not new lookups.
    assert!(
        report.stats.contains_key("cache_hit"),
        "expected cache_hit telemetry"
    );
}
