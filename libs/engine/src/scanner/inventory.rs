//! Typed estate inventory — Scanner output, Mapper input.
//!
//! Pattern: terrashift_plan.md §5 (HLD-2 box 1 → box 2 contract).
//! Constitution: Article XIII rule 4 (boundary type discipline — Inventory is
//! the storage form; the Mapper translates to MappingPlan).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Top-level scanner output.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EstateInventory {
    pub root_dir: PathBuf,
    pub files: Vec<ScannedFile>,
}

impl EstateInventory {
    /// Total number of resources across all files (for tests + audit).
    pub fn resource_count(&self) -> usize {
        self.files.iter().map(|f| f.resources.len()).sum()
    }
}

/// Entities extracted from a single `.tf` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub providers: Vec<Provider>,
    pub modules: Vec<Module>,
    pub resources: Vec<Resource>,
    pub variables: Vec<Variable>,
    pub outputs: Vec<Output>,
    pub data_sources: Vec<DataSource>,
    /// Non-fatal observations (e.g., `count` or `for_each` use, deprecated
    /// syntax) that the Mapper / Validator should consider. See clarify Q4.
    pub scan_notes: Vec<String>,
}

impl ScannedFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            providers: Vec::new(),
            modules: Vec::new(),
            resources: Vec::new(),
            variables: Vec::new(),
            outputs: Vec::new(),
            data_sources: Vec::new(),
            scan_notes: Vec::new(),
        }
    }
}

/// `provider "aws" { region = "us-east-1" ... }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub provider_type: String,
    pub alias: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub source_span: SourceSpan,
}

/// `module "vpc" { source = "../../modules/vpc" ... }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub source: Option<String>,
    pub version: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub source_span: SourceSpan,
}

/// `resource "aws_vpc" "default" { cidr_block = ... }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub resource_type: String,
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub source_span: SourceSpan,
}

/// `data "aws_caller_identity" "current" { }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub data_type: String,
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub source_span: SourceSpan,
}

/// `variable "vpc_cidr" { type = string; default = "10.0.0.0/16" }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub var_type: Option<String>,
    pub default: Option<String>,
    pub description: Option<String>,
    pub sensitive: bool,
    pub source_span: SourceSpan,
}

/// `output "vpc_id" { value = aws_vpc.default.id }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub name: String,
    pub value_expr: Option<String>,
    pub description: Option<String>,
    pub sensitive: bool,
    pub source_span: SourceSpan,
}

/// Approximate position in the source file. Stage 1: `(file, line, col)`
/// start position only. Per clarify Q3.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceSpan {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
}
