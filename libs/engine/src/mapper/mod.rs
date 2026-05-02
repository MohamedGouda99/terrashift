//! Mapper — LLM with structured output (single call, then deterministic).
//!
//! Stage 1: minimal type definitions. Just enough for P-08 Generator to
//! compile against and for tests to construct fixtures. The real Mapper
//! (LLM call, prompt template, JSON schema, cache integration) arrives in
//! S4 (P-05).
//!
//! Pattern: terrashift_plan.md §5 (agentic-vs-deterministic split — Mapper
//! is single-LLM-call structured-output, NOT an agent), §6.X (strict JSON
//! conforming to schemars-derived schema). Constitution: Article I (NOT an
//! agent; adding agentic complexity requires RFC).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

/// Output of the Mapper. Input to Validator + Generator.
///
/// Stage 1 shape; S4 (P-05) extends with: cache_key, version-pinned
/// schemas, per-resource confidence scores, fallback annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingPlan {
    /// The migration run this plan belongs to. Used to scope backups
    /// (`.terrashift/runs/{run_id}/backups/`) and audit entries.
    pub run_id: Uuid,

    /// Source provider key, e.g., "aws", "google", "azurerm".
    pub source_provider: String,

    /// Target provider key.
    pub target_provider: String,

    /// One entry per source resource. Order is the Mapper's emission order;
    /// Generator sorts by `target_addr` for byte-stable output (Article VI).
    pub resources: Vec<MappedResource>,
}

/// One source-resource → target-resource mapping. The Generator looks up a
/// template by `target_type` and feeds it `attributes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedResource {
    /// e.g., "aws_vpc.main"
    pub source_addr: String,
    /// e.g., "azurerm_virtual_network.main"
    pub target_addr: String,
    /// e.g., "azurerm_virtual_network" — first HCL block label.
    pub target_type: String,
    /// e.g., "main" — second HCL block label.
    pub target_name: String,

    /// Target-shaped attributes. `BTreeMap` for deterministic iteration
    /// order (Article VI — same input gives same output).
    pub attributes: BTreeMap<String, AttributeValue>,

    /// Other `target_addr`s this resource depends on. Stage 1 informational
    /// only; the Planner consumes this for the apply-order DAG.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// HCL-emit-friendly attribute values. Distinct from `hcl::Value` because:
/// 1. JSON-friendly serde for the Mapper-produced wire format.
/// 2. `Reference` is a raw HCL expression (e.g., `aws_vpc.main.id`) that
///    must NOT be quoted at emit time — distinguishing data from syntax.
///
/// Serde representation: **externally tagged** (the default) — variants
/// are wrapped in a single-key map, e.g. `{ "string": "10.0.0.0/16" }`,
/// `{ "reference": "aws_vpc.main.id" }`. We can't use internally-tagged
/// (`#[serde(tag = "...")]`) because the `String`/`Reference` newtype
/// variants wrap primitives, not struct/maps, and serde rejects internal
/// tagging on primitive newtypes at deserialize time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Bool(bool),
    List(Vec<AttributeValue>),
    Map(BTreeMap<String, AttributeValue>),
    /// A raw HCL expression — emitted unquoted. Examples:
    /// `"aws_vpc.main.id"`, `"var.region"`, `"data.aws_ami.example.id"`.
    Reference(String),
}

impl MappedResource {
    /// Extract a required string attribute, or return a structured error
    /// the Generator wraps into `GeneratorError::Lookup`.
    pub fn require_string(&self, attr: &str) -> Result<&str, MapperLookupError> {
        match self.attributes.get(attr) {
            Some(AttributeValue::String(s)) => Ok(s),
            Some(_) => Err(MapperLookupError::WrongType {
                addr: self.target_addr.clone(),
                attr: attr.to_string(),
                expected: "string",
            }),
            None => Err(MapperLookupError::Missing {
                addr: self.target_addr.clone(),
                attr: attr.to_string(),
            }),
        }
    }

    /// Optional string getter. None if absent or wrong type — caller decides.
    pub fn optional_string(&self, attr: &str) -> Option<&str> {
        match self.attributes.get(attr) {
            Some(AttributeValue::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Optional bool getter.
    pub fn optional_bool(&self, attr: &str) -> Option<bool> {
        match self.attributes.get(attr) {
            Some(AttributeValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Optional reference getter — returns the raw HCL expression string
    /// without quotes (e.g., `"aws_vpc.main.id"`).
    pub fn optional_reference(&self, attr: &str) -> Option<&str> {
        match self.attributes.get(attr) {
            Some(AttributeValue::Reference(r)) => Some(r.as_str()),
            _ => None,
        }
    }

    /// Optional map getter — for nested blocks like `tags = { ... }`.
    pub fn optional_map(&self, attr: &str) -> Option<&BTreeMap<String, AttributeValue>> {
        match self.attributes.get(attr) {
            Some(AttributeValue::Map(m)) => Some(m),
            _ => None,
        }
    }
}

/// Errors from `MappedResource` lookup helpers.
#[derive(Debug, Error)]
pub enum MapperLookupError {
    #[error("missing required attribute '{attr}' on resource '{addr}'")]
    Missing { addr: String, attr: String },
    #[error("attribute '{attr}' on resource '{addr}' has wrong type (expected {expected})")]
    WrongType {
        addr: String,
        attr: String,
        expected: &'static str,
    },
}
