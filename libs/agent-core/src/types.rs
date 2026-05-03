//! Core agent types: run context, proposed tool calls, decisions, approval
//! policy, retry config, agent commands, and loop result.
//!
//! Pattern: the architecture reference section 8 (the canonical agent loop kernel).
//! Source: the reference codebase (see ATTRIBUTIONS.md) (subset — only what
//! the Stage-1 + S9 surface needs).
//! Constitution: Article I (architectural restraint), II (verbatim mirror),
//! XIII rule 9 (no duplicate tool_call_id — enforced at the AgentCommand
//! resolution boundary).
//!
//! S9 (this commit) adds the surface needed by `run_agent` + `approval` +
//! `retry`: AgentCommand, ToolApprovalAction, ToolApprovalPolicy,
//! RetryConfig, AgentLoopConfig, AgentLoopResult, AgentLoopReason.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Per-run identity passed into every `ToolExecutor::execute_tool_call` and
/// every `AgentHook` lifecycle method.
///
/// Verbatim from the reference codebase (see ATTRIBUTIONS.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunContext {
    pub run_id: Uuid,
    pub session_id: Uuid,
}

/// A tool call proposed by the LLM, before approval and execution.
///
/// Verbatim from the reference codebase (see ATTRIBUTIONS.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// User decision on a proposed tool call, returned by the approval flow.
///
/// Verbatim from the reference codebase (see ATTRIBUTIONS.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolDecision {
    Accept,
    Reject,
    CustomResult { content: String },
}

// ---------------------------------------------------------------------
// S9 additions — approval policy
// ---------------------------------------------------------------------

/// What the approval policy says to do for a given tool call.
///
/// Verbatim from the reference codebase (see ATTRIBUTIONS.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalAction {
    /// Auto-accept without user prompt.
    Approve,
    /// Auto-reject without user prompt.
    Deny,
    /// Surface to the user for explicit decision.
    Ask,
}

/// Per-tool-call policy resolution. Stage 2 wires this from the operator's
/// profile (Article V, V-of-XIII, or both — see `pre-flight.md` decision 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolApprovalPolicy {
    /// No auto-decisions — every tool call is `Ask`. The default.
    None,
    /// Auto-accept everything (e.g., for hermetic eval runs).
    AcceptAll,
    /// Auto-reject everything (e.g., for read-only dry-run).
    DenyAll,
    /// Per-tool rules + a default for tools not in the map.
    Custom {
        rules: HashMap<String, ToolApprovalAction>,
        default: ToolApprovalAction,
    },
}

impl ToolApprovalPolicy {
    /// Resolve the policy decision for a specific tool call. The
    /// `_arguments` parameter is reserved for future arg-pattern matching
    /// (e.g., shell-tool-approvals integration); Stage-2 narrows to
    /// name-only matching.
    pub fn action_for(
        &self,
        tool_name: &str,
        _arguments: Option<&serde_json::Value>,
    ) -> ToolApprovalAction {
        match self {
            ToolApprovalPolicy::None => ToolApprovalAction::Ask,
            ToolApprovalPolicy::AcceptAll => ToolApprovalAction::Approve,
            ToolApprovalPolicy::DenyAll => ToolApprovalAction::Deny,
            ToolApprovalPolicy::Custom { rules, default } => {
                rules.get(tool_name).copied().unwrap_or(*default)
            }
        }
    }
}

// ---------------------------------------------------------------------
// S9 additions — retry config
// ---------------------------------------------------------------------

/// Retry policy for transient LLM errors (rate limits, 5xx, timeouts).
///
/// Verbatim from the reference codebase (see ATTRIBUTIONS.md).
#[derive(Debug, Clone, PartialEq)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            initial_backoff_ms: 2_000,
            max_backoff_ms: 30_000,
            multiplier: 2.0,
        }
    }
}

// ---------------------------------------------------------------------
// S9 additions — agent commands (steering primitives)
// ---------------------------------------------------------------------

/// Commands the host can apply to the agent loop. Stage 2 narrows to
/// approval-resolution commands; Stage 5 (S13 detached mode) extends with
/// `Steer`, `FollowUp`, `SwitchModel`, `Abort`.
///
/// Pattern: the reference codebase (see ATTRIBUTIONS.md) (narrowed).
#[derive(Debug, Clone)]
pub enum AgentCommand {
    /// Resolve a single proposed tool call.
    ResolveTool {
        tool_call_id: String,
        decision: ToolDecision,
    },
    /// Bulk-resolve multiple tool calls in one shot.
    ResolveTools {
        decisions: HashMap<String, ToolDecision>,
    },
}

// ---------------------------------------------------------------------
// S9 additions — loop config + result
// ---------------------------------------------------------------------

/// Bounded configuration for a single `run_agent` invocation.
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    /// Hard cap on the number of LLM-call → tool-execution turns.
    /// `0` is invalid (kernel returns `AgentError::InvalidCommand`).
    pub max_turns: usize,
    pub retry: RetryConfig,
    pub approval_policy: ToolApprovalPolicy,
    /// When the conversation token count crosses this threshold, the
    /// kernel runs `CompactionEngine::compact` before the next LLM call.
    /// Stage 2 ships `PassthroughCompactionEngine` (no-op) so this is
    /// a structural seam, not a functional gate.
    pub compaction_threshold_tokens: usize,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_turns: 8,
            retry: RetryConfig::default(),
            approval_policy: ToolApprovalPolicy::None,
            // 70k matches the reference's default for Anthropic 200k context;
            // proper per-model thresholds arrive in Stage 5+ when the
            // tier-aware router widens the trait surface.
            compaction_threshold_tokens: 70_000,
        }
    }
}

/// Why the loop terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLoopReason {
    /// LLM produced a final answer (no tool calls in last turn).
    FinalAnswerReached,
    /// `max_turns` was exhausted.
    MaxTurnsReached,
    /// Approval state machine has at least one `PendingUserDecision`
    /// entry — kernel returned control so the host can collect a
    /// user response (S13 detached mode).
    WaitingForApproval,
    /// LLM retry budget exhausted.
    LlmRetryExhausted,
}

/// Terminal state of one `run_agent` invocation.
#[derive(Debug, Clone)]
pub struct AgentLoopResult {
    pub reason: AgentLoopReason,
    /// Final assistant text (empty when `reason = WaitingForApproval`).
    pub final_text: String,
    pub turns_executed: usize,
    /// Pending tool-call ids when `reason = WaitingForApproval`. Empty
    /// for other terminal states.
    pub pending_tool_calls: Vec<String>,
}
