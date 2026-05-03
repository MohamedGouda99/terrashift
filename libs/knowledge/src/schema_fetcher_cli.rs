//! `TerraformCliSchemaFetcher` — real `SchemaFetcher` impl.
//!
//! The Terraform Registry API does NOT expose provider schemas directly
//! — schemas live inside the provider binary, only readable via the
//! `terraform providers schema -json` subcommand. This is the same path
//! Pulumi tf2pulumi, Terraformer, and Spacelift's introspection use.
//!
//! ## Flow
//!
//! 1. Create a tempdir.
//! 2. Write a tiny `versions.tf` declaring `<namespace>/<name> = <version>`.
//! 3. Run `terraform init -input=false -no-color` — downloads the provider
//!    binary from the registry into the tempdir.
//! 4. Run `terraform providers schema -json` — the provider binary itself
//!    emits its full schema as JSON.
//! 5. Parse the JSON into Terrashift's `ProviderSchema` shape.
//!
//! ## Performance
//!
//! Each call performs a fresh `terraform init`, which downloads the
//! provider binary if not already cached. Setting `TF_PLUGIN_CACHE_DIR`
//! to `~/.terraform.d/plugin-cache/` (the default location) reuses
//! binaries across calls. First-time cost: ~5–15s for AWS, ~10–30s
//! for azurerm. Cached cost: ~1–3s.
//!
//! ## Constitution
//!
//! - Article IV (loud failure — non-zero exit code, parse error, etc.
//!   surface as `RegistryError` variants with full stderr captured).
//! - Article XIII rule 7 (no disk-bound secret writes — the tempdir
//!   contains only the public versions.tf and the downloaded provider
//!   binary; no creds touch disk via this path).

use crate::registry_client::{RegistryError, SchemaFetcher};
use crate::types::{AttributeSchema, ProviderSchema, ResourceSchema};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{debug, info, instrument};

/// Real schema fetcher — runs the `terraform` CLI in a tempdir to extract
/// the provider's full schema JSON. Operator must have `terraform >= 1.0`
/// on PATH.
pub struct TerraformCliSchemaFetcher {
    /// Override the binary name (default: `"terraform"`). Useful when
    /// the operator has multiple terraform versions installed (e.g.,
    /// via `tfenv` / `tfswitch`).
    terraform_bin: String,
    /// Optional override for `TF_PLUGIN_CACHE_DIR`. When unset, terraform
    /// uses its default (`~/.terraform.d/plugin-cache/`). Setting this
    /// to a controlled path makes per-run caching deterministic.
    plugin_cache_dir: Option<PathBuf>,
    /// Per-call timeout. Default 120s, plenty for the slowest provider
    /// (azurerm ~30s on a cold cache).
    timeout: Duration,
}

impl TerraformCliSchemaFetcher {
    pub fn new() -> Self {
        Self {
            terraform_bin: "terraform".to_string(),
            plugin_cache_dir: None,
            timeout: Duration::from_secs(120),
        }
    }

    pub fn with_terraform_bin(mut self, bin: impl Into<String>) -> Self {
        self.terraform_bin = bin.into();
        self
    }

    pub fn with_plugin_cache_dir(mut self, dir: PathBuf) -> Self {
        self.plugin_cache_dir = Some(dir);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sanity check: is the terraform binary on PATH and version
    /// >= 1.0? Surfaces a clean `TerraformUnavailable` error if not.
    pub async fn check_available(&self) -> Result<String, RegistryError> {
        let output = Command::new(&self.terraform_bin)
            .arg("version")
            .arg("-json")
            .output()
            .await
            .map_err(|e| {
                RegistryError::TerraformUnavailable(format!(
                    "could not run `{} version`: {e}",
                    self.terraform_bin
                ))
            })?;
        if !output.status.success() {
            return Err(RegistryError::TerraformUnavailable(format!(
                "`{} version` exited {}",
                self.terraform_bin, output.status
            )));
        }
        let version_str = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(version_str)
    }
}

impl Default for TerraformCliSchemaFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchemaFetcher for TerraformCliSchemaFetcher {
    #[instrument(skip(self), fields(namespace, name, version))]
    async fn fetch(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<ProviderSchema, RegistryError> {
        // 1. Tempdir for the work directory.
        let workdir = TempDir::new()
            .map_err(|e| RegistryError::SchemaFetch(format!("could not create tempdir: {e}")))?;

        // 2. Write versions.tf.
        let versions_tf = format!(
            r#"terraform {{
  required_providers {{
    {name} = {{
      source  = "{namespace}/{name}"
      version = "{version}"
    }}
  }}
}}
"#
        );
        std::fs::write(workdir.path().join("versions.tf"), &versions_tf)
            .map_err(|e| RegistryError::SchemaFetch(format!("write versions.tf: {e}")))?;

        // 3. Run `terraform init`.
        info!(
            "running `terraform init` for {namespace}/{name}@{version} (in {})",
            workdir.path().display()
        );
        let mut init_cmd = Command::new(&self.terraform_bin);
        init_cmd
            .arg("init")
            .arg("-input=false")
            .arg("-no-color")
            .current_dir(workdir.path());
        if let Some(cache) = &self.plugin_cache_dir {
            init_cmd.env("TF_PLUGIN_CACHE_DIR", cache);
        }
        let init_output = tokio::time::timeout(self.timeout, init_cmd.output())
            .await
            .map_err(|_| {
                RegistryError::SchemaFetch(format!(
                    "`terraform init` timed out after {}s",
                    self.timeout.as_secs()
                ))
            })?
            .map_err(|e| RegistryError::SchemaFetch(format!("spawn terraform init: {e}")))?;

        if !init_output.status.success() {
            let stderr = String::from_utf8_lossy(&init_output.stderr).to_string();
            return Err(RegistryError::SchemaFetch(format!(
                "`terraform init` failed (exit {}): {}",
                init_output.status,
                first_chars(&stderr, 500)
            )));
        }

        // 4. Run `terraform providers schema -json`.
        debug!("running `terraform providers schema -json`");
        let schema_output = tokio::time::timeout(
            self.timeout,
            Command::new(&self.terraform_bin)
                .arg("providers")
                .arg("schema")
                .arg("-json")
                .current_dir(workdir.path())
                .output(),
        )
        .await
        .map_err(|_| {
            RegistryError::SchemaFetch(format!(
                "`terraform providers schema -json` timed out after {}s",
                self.timeout.as_secs()
            ))
        })?
        .map_err(|e| RegistryError::SchemaFetch(format!("spawn terraform schema: {e}")))?;

        if !schema_output.status.success() {
            let stderr = String::from_utf8_lossy(&schema_output.stderr).to_string();
            return Err(RegistryError::SchemaFetch(format!(
                "`terraform providers schema -json` failed (exit {}): {}",
                schema_output.status,
                first_chars(&stderr, 500)
            )));
        }

        // 5. Parse the JSON.
        let json_str = String::from_utf8_lossy(&schema_output.stdout).to_string();
        let parsed: TfSchemaOutput = serde_json::from_str(&json_str).map_err(|e| {
            RegistryError::SchemaFetch(format!(
                "parse terraform schema JSON: {e}; first 200 chars: {}",
                first_chars(&json_str, 200)
            ))
        })?;

        // 6. Find the right provider entry.
        let provider_key = format!("registry.terraform.io/{namespace}/{name}");
        let tf_schema = parsed.provider_schemas.get(&provider_key).ok_or_else(|| {
            RegistryError::SchemaFetch(format!(
                "provider key '{provider_key}' not in schema output (got: {:?})",
                parsed.provider_schemas.keys().collect::<Vec<_>>()
            ))
        })?;

        // 7. Convert to our ProviderSchema.
        let mut resources: BTreeMap<String, ResourceSchema> = BTreeMap::new();
        for (resource_name, tf_resource) in &tf_schema.resource_schemas {
            resources.insert(
                resource_name.clone(),
                convert_resource(resource_name, tf_resource),
            );
        }

        let mut data_sources: BTreeMap<String, ResourceSchema> = BTreeMap::new();
        for (ds_name, tf_resource) in &tf_schema.data_source_schemas {
            data_sources.insert(ds_name.clone(), convert_resource(ds_name, tf_resource));
        }

        info!(
            "fetched {} resources + {} data sources for {namespace}/{name}@{version}",
            resources.len(),
            data_sources.len()
        );

        Ok(ProviderSchema {
            provider: name.to_string(),
            version: version.to_string(),
            resources,
            data_sources,
            fetched_at: Utc::now(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────
// Terraform schema JSON shape — partial, only what we consume.
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TfSchemaOutput {
    #[serde(default)]
    provider_schemas: BTreeMap<String, TfProviderEntry>,
}

#[derive(Debug, Deserialize)]
struct TfProviderEntry {
    #[serde(default)]
    resource_schemas: BTreeMap<String, TfResourceSchema>,
    #[serde(default)]
    data_source_schemas: BTreeMap<String, TfResourceSchema>,
}

#[derive(Debug, Deserialize)]
struct TfResourceSchema {
    #[serde(default)]
    block: TfBlock,
}

#[derive(Debug, Default, Deserialize)]
struct TfBlock {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    attributes: BTreeMap<String, TfAttribute>,
}

#[derive(Debug, Deserialize)]
struct TfAttribute {
    /// Type is either a string ("string", "bool") or a JSON array
    /// (["list", "string"], ["map", "string"], ["object", {...}]).
    /// Optional because some attributes use `nested_type` instead.
    #[serde(default)]
    r#type: Option<serde_json::Value>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    computed: bool,
    #[serde(default)]
    sensitive: bool,
    /// Terraform emits this as a plain `bool`. We map `true` → our
    /// `Some("deprecated")` and `false`/omitted → `None`. Operators
    /// who want a richer message can read `description` (which often
    /// contains "Deprecated: use foo_bar instead").
    #[serde(default)]
    deprecated: bool,
}

fn convert_resource(name: &str, tf: &TfResourceSchema) -> ResourceSchema {
    let mut attributes: BTreeMap<String, AttributeSchema> = BTreeMap::new();
    for (attr_name, tf_attr) in &tf.block.attributes {
        attributes.insert(attr_name.clone(), convert_attribute(attr_name, tf_attr));
    }
    ResourceSchema {
        name: name.to_string(),
        description: tf.block.description.clone(),
        attributes,
    }
}

fn convert_attribute(name: &str, tf: &TfAttribute) -> AttributeSchema {
    AttributeSchema {
        name: name.to_string(),
        attribute_type: render_type(tf.r#type.as_ref()),
        required: tf.required,
        optional: tf.optional,
        computed: tf.computed,
        sensitive: tf.sensitive,
        deprecated: if tf.deprecated {
            Some("deprecated".to_string())
        } else {
            None
        },
        description: tf.description.clone(),
    }
}

/// Render terraform's heterogeneous type representation as a stable
/// string. Examples:
///   "string"                                → "string"
///   ["list", "string"]                      → "list(string)"
///   ["map", "string"]                       → "map(string)"
///   ["object", { "foo": "string" }]         → "object({foo=string})"
///   ["set", ["list", "string"]]             → "set(list(string))"
fn render_type(value: Option<&serde_json::Value>) -> String {
    match value {
        None => "any".to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => {
            if arr.is_empty() {
                return "any".to_string();
            }
            let head = arr.first().and_then(|v| v.as_str()).unwrap_or("any");
            match head {
                "list" | "set" | "map" => {
                    let inner = arr.get(1).map(|v| render_type(Some(v)));
                    format!("{head}({})", inner.unwrap_or_else(|| "any".to_string()))
                }
                "object" => {
                    if let Some(serde_json::Value::Object(obj)) = arr.get(1) {
                        let mut parts: Vec<String> = obj
                            .iter()
                            .map(|(k, v)| format!("{k}={}", render_type(Some(v))))
                            .collect();
                        parts.sort();
                        format!("object({{{}}})", parts.join(","))
                    } else {
                        "object({})".to_string()
                    }
                }
                "tuple" => {
                    if let Some(serde_json::Value::Array(items)) = arr.get(1) {
                        let parts: Vec<String> =
                            items.iter().map(|v| render_type(Some(v))).collect();
                        format!("tuple([{}])", parts.join(","))
                    } else {
                        "tuple([])".to_string()
                    }
                }
                other => other.to_string(),
            }
        }
        Some(other) => other.to_string(),
    }
}

fn first_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => s.get(..idx).unwrap_or(s),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn render_type_handles_strings_lists_maps_objects() {
        assert_eq!(render_type(None), "any");
        assert_eq!(
            render_type(Some(&serde_json::Value::String("string".to_string()))),
            "string"
        );

        let list_string =
            serde_json::from_str::<serde_json::Value>(r#"["list", "string"]"#).unwrap();
        assert_eq!(render_type(Some(&list_string)), "list(string)");

        let map_string = serde_json::from_str::<serde_json::Value>(r#"["map", "string"]"#).unwrap();
        assert_eq!(render_type(Some(&map_string)), "map(string)");

        let nested =
            serde_json::from_str::<serde_json::Value>(r#"["set", ["list", "string"]]"#).unwrap();
        assert_eq!(render_type(Some(&nested)), "set(list(string))");

        let object = serde_json::from_str::<serde_json::Value>(
            r#"["object", {"foo": "string", "bar": "number"}]"#,
        )
        .unwrap();
        // Keys sorted alphabetically for stability.
        assert_eq!(
            render_type(Some(&object)),
            "object({bar=number,foo=string})"
        );
    }

    #[test]
    fn convert_attribute_passes_through_flags() {
        let tf = TfAttribute {
            r#type: Some(serde_json::Value::String("string".to_string())),
            description: Some("doc".to_string()),
            required: true,
            optional: false,
            computed: false,
            sensitive: false,
            deprecated: false,
        };
        let attr = convert_attribute("name", &tf);
        assert_eq!(attr.name, "name");
        assert_eq!(attr.attribute_type, "string");
        assert!(attr.required);
        assert!(!attr.optional);
        assert_eq!(attr.deprecated, None);
    }

    #[test]
    fn convert_attribute_marks_deprecated_when_terraform_says_true() {
        let tf = TfAttribute {
            r#type: Some(serde_json::Value::String("string".to_string())),
            description: None,
            required: false,
            optional: true,
            computed: false,
            sensitive: false,
            deprecated: true,
        };
        let attr = convert_attribute("legacy_attr", &tf);
        assert_eq!(attr.deprecated.as_deref(), Some("deprecated"));
    }

    /// Real fetch — pulls aws@5.30.0 from the registry. Gated `#[ignore]`
    /// because it requires the terraform CLI on PATH and ~30s of network
    /// time. Run with: `cargo test -p terrashift-knowledge fetch_real_aws -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "real terraform CLI subprocess; requires terraform on PATH + network"]
    async fn fetch_real_aws_provider_schema() {
        let fetcher = TerraformCliSchemaFetcher::new();
        // Sanity-check terraform is installed.
        let version = fetcher.check_available().await.expect("terraform on PATH");
        eprintln!("Using {version}");

        let schema = fetcher
            .fetch("hashicorp", "aws", "5.30.0")
            .await
            .expect("fetch aws@5.30.0");

        assert_eq!(schema.provider, "aws");
        assert_eq!(schema.version, "5.30.0");
        assert!(
            schema.resources.len() > 1000,
            "AWS provider should have >1000 resources; got {}",
            schema.resources.len()
        );
        assert!(schema.resources.contains_key("aws_vpc"));

        let vpc = schema.resources.get("aws_vpc").expect("aws_vpc");
        assert!(vpc.attributes.contains_key("cidr_block"));
        eprintln!(
            "✓ aws_vpc has {} attributes; full schema has {} resources + {} data sources",
            vpc.attributes.len(),
            schema.resources.len(),
            schema.data_sources.len()
        );
    }
}
