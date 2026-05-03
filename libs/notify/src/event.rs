//! Notification event taxonomy.
//!
//! These are the events the Terrashift migration runtime emits to
//! operators when running in detached / agentic mode. Each event has
//! a severity and a builder method on `NotificationEvent`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational (`MigrationStarted`, `ResourceMapped`). Slack
    /// posts as a thread reply or muted color.
    Info,
    /// Operator action requested (`ApprovalRequired`). Slack posts
    /// with attention color.
    Action,
    /// Migration succeeded (`MigrationCompleted`). Green color.
    Success,
    /// Operator-visible warning (`CostAlert`, `RecoveryPartial`).
    /// Yellow color.
    Warn,
    /// Migration failed (`MigrationFailed`, `RecoveryExhausted`).
    /// Red color.
    Error,
}

impl Severity {
    /// Slack attachment color. Matches Slack's standard palette so
    /// rendering is consistent with other tools the operator uses.
    pub fn slack_color(self) -> &'static str {
        match self {
            Severity::Info => "#94A3B8",
            Severity::Action => "#F7931E",
            Severity::Success => "#22C55E",
            Severity::Warn => "#FBBF24",
            Severity::Error => "#EF4444",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Severity::Info => ":information_source:",
            Severity::Action => ":eyes:",
            Severity::Success => ":white_check_mark:",
            Severity::Warn => ":warning:",
            Severity::Error => ":x:",
        }
    }
}

/// What the migration runtime emits. `run_id` ties an event to a
/// specific migration run so operators can correlate across channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub run_id: Uuid,
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    /// Optional structured fields to render as Slack key-value rows.
    /// Order is preserved (we use a `Vec` instead of a `HashMap`).
    #[serde(default)]
    pub fields: Vec<(String, String)>,
}

impl NotificationEvent {
    pub fn new(
        run_id: Uuid,
        severity: Severity,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            run_id,
            severity,
            title: title.into(),
            body: body.into(),
            timestamp: Utc::now(),
            fields: Vec::new(),
        }
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((key.into(), value.into()));
        self
    }

    // ---- Canonical event builders -----------------------------------

    pub fn migration_started(run_id: Uuid, source: &str, target: &str, n_resources: usize) -> Self {
        Self::new(
            run_id,
            Severity::Info,
            "Migration started",
            format!("Starting migration: {source} → {target} ({n_resources} resources)"),
        )
        .with_field("source", source)
        .with_field("target", target)
        .with_field("resources", n_resources.to_string())
    }

    pub fn migration_completed(run_id: Uuid, n_resources: usize, duration_seconds: u64) -> Self {
        Self::new(
            run_id,
            Severity::Success,
            "Migration completed",
            format!("Migrated {n_resources} resources in {duration_seconds}s"),
        )
        .with_field("resources", n_resources.to_string())
        .with_field("duration_s", duration_seconds.to_string())
    }

    pub fn migration_failed(run_id: Uuid, phase: &str, reason: &str) -> Self {
        Self::new(
            run_id,
            Severity::Error,
            "Migration failed",
            format!("Phase '{phase}' failed: {reason}"),
        )
        .with_field("phase", phase)
        .with_field("reason", reason)
    }

    pub fn approval_required(run_id: Uuid, n_pending: usize, summary: &str) -> Self {
        Self::new(
            run_id,
            Severity::Action,
            "Approval required",
            format!("{n_pending} tool call(s) need approval: {summary}"),
        )
        .with_field("pending", n_pending.to_string())
    }

    pub fn cost_alert(run_id: Uuid, target_usd: f64, actual_usd: f64, delta_pct: f64) -> Self {
        Self::new(
            run_id,
            Severity::Warn,
            "Cost target exceeded",
            format!("Target ${target_usd:.2}/mo, actual ${actual_usd:.2}/mo ({delta_pct:+.1}%)"),
        )
        .with_field("target_usd", format!("{target_usd:.2}"))
        .with_field("actual_usd", format!("{actual_usd:.2}"))
        .with_field("delta_pct", format!("{delta_pct:+.1}"))
    }

    pub fn recovery_exhausted(run_id: Uuid, iterations: usize, unresolved: usize) -> Self {
        Self::new(
            run_id,
            Severity::Error,
            "Recovery agent exhausted",
            format!(
                "Recovery hit {iterations} iterations with {unresolved} validator error(s) unresolved"
            ),
        )
        .with_field("iterations", iterations.to_string())
        .with_field("unresolved_errors", unresolved.to_string())
    }
}
