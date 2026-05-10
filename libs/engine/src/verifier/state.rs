// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Typed deserialization of `terraform show -json` output.
//!
//! We parse only the subset Verifier needs:
//! - `format_version` (gate on "1.x")
//! - `values.root_module.resources` (and recursively `child_modules`)
//! - `address`, `mode`, `type`, `name`, `provider_name` per resource
//!
//! Unknown fields are tolerated — terraform's JSON output evolves and
//! locking ourselves to today's exact schema would force us to rev
//! Verifier on every minor terraform release.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TerraformState {
    /// Stable across Terraform 1.x; Stage 1 gates on `starts_with("1.")`.
    pub format_version: String,
    /// `null` when the state file is empty (right after `terraform init`
    /// before any resource has been applied). Verifier treats this as
    /// "zero resources in state".
    #[serde(default)]
    pub values: Option<ValuesBlock>,
}

#[derive(Debug, Deserialize)]
pub struct ValuesBlock {
    pub root_module: RootModule,
}

#[derive(Debug, Deserialize)]
pub struct RootModule {
    #[serde(default)]
    pub resources: Vec<StateResource>,
    /// Recursive — modules can contain modules.
    #[serde(default)]
    pub child_modules: Vec<ChildModule>,
}

#[derive(Debug, Deserialize)]
pub struct ChildModule {
    /// Unused at Stage 1 (single-region, no module nesting expected).
    /// Kept on the type so unknown-input tolerance covers the field.
    #[serde(default)]
    #[allow(dead_code)]
    pub address: Option<String>,
    #[serde(default)]
    pub resources: Vec<StateResource>,
    #[serde(default)]
    pub child_modules: Vec<ChildModule>,
}

#[derive(Debug, Deserialize)]
pub struct StateResource {
    /// e.g., "aws_vpc.main" or "module.foo.aws_vpc.main".
    pub address: String,
    /// "managed" | "data". Stage 1 only emits managed; data shows up as
    /// extras (warnings, not errors).
    #[serde(default = "default_mode")]
    pub mode: String,
    /// e.g., "aws_vpc". Serde rename: `type` is reserved.
    #[serde(rename = "type")]
    pub resource_type: String,
    /// e.g., "main". Carried for forward-compatibility (Stage 4 attribute
    /// diff will use it for human-readable error context); not exercised
    /// by Stage 1 logic, which keys on `address` directly.
    #[serde(default)]
    #[allow(dead_code)]
    pub name: String,
    /// e.g., "registry.terraform.io/hashicorp/aws". Verifier extracts
    /// the last path segment as the canonical provider name.
    pub provider_name: String,
}

fn default_mode() -> String {
    "managed".to_string()
}

/// Walk `root_module` + every `child_module` recursively, collecting
/// every `StateResource` into a single flat `Vec<&StateResource>`.
///
/// Order: root first, then DFS into child modules. Stable for a given
/// state file, but Verifier sorts by address before comparing so the
/// order doesn't reach the report.
pub fn flatten_resources(state: &TerraformState) -> Vec<&StateResource> {
    let mut out = Vec::new();
    let Some(values) = state.values.as_ref() else {
        return out;
    };
    flatten_root(&values.root_module, &mut out);
    out
}

fn flatten_root<'a>(root: &'a RootModule, out: &mut Vec<&'a StateResource>) {
    out.extend(root.resources.iter());
    for child in &root.child_modules {
        flatten_child(child, out);
    }
}

fn flatten_child<'a>(child: &'a ChildModule, out: &mut Vec<&'a StateResource>) {
    out.extend(child.resources.iter());
    for grandchild in &child.child_modules {
        flatten_child(grandchild, out);
    }
}

/// Extract the canonical provider name from terraform's `provider_name`
/// URI form. Examples:
///   "registry.terraform.io/hashicorp/aws" -> "aws"
///   "registry.terraform.io/hashicorp/azurerm" -> "azurerm"
///   "hashicorp/google" -> "google" (older format)
///   "aws" -> "aws" (degenerate)
pub fn canonical_provider_name(provider_uri: &str) -> &str {
    provider_uri.rsplit('/').next().unwrap_or(provider_uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_STATE: &str = r#"{
        "format_version": "1.0",
        "values": {
            "root_module": {
                "resources": [
                    {
                        "address": "aws_vpc.main",
                        "mode": "managed",
                        "type": "aws_vpc",
                        "name": "main",
                        "provider_name": "registry.terraform.io/hashicorp/aws"
                    }
                ]
            }
        }
    }"#;

    #[test]
    fn parse_minimal_state() {
        let state: TerraformState = serde_json::from_str(MINIMAL_STATE).expect("parse");
        assert_eq!(state.format_version, "1.0");
        let flat = flatten_resources(&state);
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].address, "aws_vpc.main");
        assert_eq!(flat[0].resource_type, "aws_vpc");
    }

    #[test]
    fn parse_state_with_no_values() {
        let json = r#"{ "format_version": "1.0" }"#;
        let state: TerraformState = serde_json::from_str(json).expect("parse");
        assert!(state.values.is_none());
        assert_eq!(flatten_resources(&state).len(), 0);
    }

    #[test]
    fn parse_state_with_child_modules_recursive() {
        let json = r#"{
            "format_version": "1.0",
            "values": {
                "root_module": {
                    "resources": [
                        { "address": "a", "type": "t", "name": "a", "provider_name": "registry.terraform.io/hashicorp/aws" }
                    ],
                    "child_modules": [
                        {
                            "address": "module.foo",
                            "resources": [
                                { "address": "module.foo.b", "type": "t", "name": "b", "provider_name": "registry.terraform.io/hashicorp/aws" }
                            ],
                            "child_modules": [
                                {
                                    "address": "module.foo.module.bar",
                                    "resources": [
                                        { "address": "module.foo.module.bar.c", "type": "t", "name": "c", "provider_name": "registry.terraform.io/hashicorp/aws" }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            }
        }"#;
        let state: TerraformState = serde_json::from_str(json).expect("parse");
        let flat = flatten_resources(&state);
        assert_eq!(flat.len(), 3);
        let addrs: Vec<&str> = flat.iter().map(|r| r.address.as_str()).collect();
        assert!(addrs.contains(&"a"));
        assert!(addrs.contains(&"module.foo.b"));
        assert!(addrs.contains(&"module.foo.module.bar.c"));
    }

    #[test]
    fn parse_tolerates_unknown_fields() {
        let json = r#"{
            "format_version": "1.0",
            "terraform_version": "1.10.0",
            "planned_values": {},
            "values": {
                "root_module": {
                    "resources": [
                        {
                            "address": "aws_vpc.main",
                            "mode": "managed",
                            "type": "aws_vpc",
                            "name": "main",
                            "provider_name": "registry.terraform.io/hashicorp/aws",
                            "schema_version": 0,
                            "values": { "cidr_block": "10.0.0.0/16" },
                            "sensitive_values": {}
                        }
                    ]
                }
            }
        }"#;
        let state: TerraformState = serde_json::from_str(json).expect("parse");
        assert_eq!(flatten_resources(&state).len(), 1);
    }

    #[test]
    fn canonical_provider_name_handles_full_uri() {
        assert_eq!(
            canonical_provider_name("registry.terraform.io/hashicorp/aws"),
            "aws"
        );
        assert_eq!(canonical_provider_name("hashicorp/google"), "google");
        assert_eq!(canonical_provider_name("azurerm"), "azurerm");
    }

    #[test]
    fn parse_garbage_returns_err() {
        let bad = "{not json";
        let result: Result<TerraformState, _> = serde_json::from_str(bad);
        assert!(result.is_err());
    }
}
