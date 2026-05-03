//! Validator — Article III enforcement gate.
//!
//! Pattern: Terrashift-specific (no upstream counterpart — migration domain).
//! Closest analog is `the architecture reference §27` (single redaction enforcement
//! point), which we mirror in spirit: one place where the AI-safety
//! invariant gets enforced, bypassing it is a CI failure.
//!
//! Constitution: Article III (every LLM-emitted attribute checked against
//!               the live target-provider schema before HCL emit;
//!               hallucinations are a build break, not a runtime warning),
//!               Article IV (validation errors are loud + named),
//!               Article XIII rule 3 (no unwrap/expect/string-slice).
//!
//! ## Public API
//!
//! ```ignore
//! use terrashift_engine::validator::Validator;
//! use terrashift_knowledge::KnowledgeService;
//! use std::sync::Arc;
//!
//! # async fn doctest(plan: terrashift_engine::mapper::MappingPlan, knowledge: Arc<KnowledgeService>) {
//! let v = Validator::new(knowledge);
//! let report = v.validate(&plan, "5.30.0").await.unwrap();
//! if !report.passed {
//!     for err in &report.errors { eprintln!("BLOCK: {err}"); }
//!     std::process::exit(1);
//! }
//! for warn in &report.warnings { eprintln!("WARN: {warn}"); }
//! # }
//! ```
//!
//! ## Where Validator sits in the pipeline
//!
//! `Mapper → Validator → Generator`. Bypassing `Validator` is a CI failure
//! (Article III). Stage 1 has no real Mapper yet, so the deterministic
//! happy path uses pre-curated `MappingPlan`s — but the gate is built so
//! S4 can wire it as soon as Mapper produces output.

pub mod errors;
pub mod report;

pub use errors::ValidatorError;
pub use report::{ValidationError, ValidationReport, ValidationWarning};

use crate::mapper::MappingPlan;
use std::sync::Arc;
use terrashift_knowledge::KnowledgeService;

/// Single Article III enforcement point.
///
/// Holds an `Arc<KnowledgeService>` to fetch target-provider schemas.
/// Stateless apart from that handle; safe to share across threads.
pub struct Validator {
    knowledge: Arc<KnowledgeService>,
}

impl Validator {
    pub fn new(knowledge: Arc<KnowledgeService>) -> Self {
        Self { knowledge }
    }

    /// Walk every `MappedResource` in `plan`, check each against the
    /// `ResourceSchema` for `(plan.target_provider, target_version)`.
    /// Aggregate findings; never fail-fast.
    ///
    /// Returns `Err` only on infrastructure failure (couldn't fetch the
    /// schema). Per-resource validation issues land in
    /// `ValidationReport.{errors,warnings}` instead.
    pub async fn validate(
        &self,
        plan: &MappingPlan,
        target_version: &str,
    ) -> Result<ValidationReport, ValidatorError> {
        tracing::debug!(
            run_id = %plan.run_id,
            target_provider = %plan.target_provider,
            target_version,
            n_resources = plan.resources.len(),
            "Validator::validate start"
        );

        let schema = self
            .knowledge
            .fetch_schema(&plan.target_provider, target_version)
            .await?;

        let mut errors: Vec<ValidationError> = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();

        for resource in &plan.resources {
            // (1) Resource-type existence check.
            let resource_schema = match schema.resources.get(&resource.target_type) {
                Some(rs) => rs,
                None => {
                    errors.push(ValidationError::UnknownResourceType {
                        addr: resource.target_addr.clone(),
                        target_type: resource.target_type.clone(),
                    });
                    continue; // No schema → no further per-attr checks possible.
                }
            };

            // (2) Per-attribute existence + computed-warning checks.
            for attr_name in resource.attributes.keys() {
                match resource_schema.attributes.get(attr_name) {
                    None => errors.push(ValidationError::UnknownAttribute {
                        addr: resource.target_addr.clone(),
                        attr: attr_name.clone(),
                        target_type: resource.target_type.clone(),
                    }),
                    Some(attr_schema) => {
                        if let Some(since) = &attr_schema.deprecated {
                            warnings.push(ValidationWarning::DeprecatedAttribute {
                                addr: resource.target_addr.clone(),
                                attr: attr_name.clone(),
                                since: since.clone(),
                            });
                        }
                        // Computed && !optional means strictly read-only.
                        // Real Terraform also has computed+optional fields
                        // (e.g., `aws_instance.subnet_id` — caller may set
                        // OR provider may compute). Gating on `!optional`
                        // avoids false-positive warnings on those.
                        if attr_schema.computed && !attr_schema.optional {
                            warnings.push(ValidationWarning::SetComputedAttribute {
                                addr: resource.target_addr.clone(),
                                attr: attr_name.clone(),
                                target_type: resource.target_type.clone(),
                            });
                        }
                    }
                }
            }

            // (3) Required-attribute presence check.
            for (attr_name, attr_schema) in &resource_schema.attributes {
                if attr_schema.required && !resource.attributes.contains_key(attr_name) {
                    errors.push(ValidationError::MissingRequiredAttribute {
                        addr: resource.target_addr.clone(),
                        attr: attr_name.clone(),
                        target_type: resource.target_type.clone(),
                    });
                }
            }
        }

        let report = ValidationReport::new(errors, warnings);
        tracing::info!(
            passed = report.passed,
            errors = report.errors.len(),
            warnings = report.warnings.len(),
            "Validator::validate done"
        );
        Ok(report)
    }
}
