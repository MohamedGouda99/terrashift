// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Stage 1 resource template registry.
//!
//! Each template is a pure function: `MappedResource → Result<hcl::Block>`.
//! No LLM in the happy path (Article I — deterministic-first). Template
//! miss → loud `Err(GeneratorError::TemplateMiss)` (Article IV).
//!
//! Stage 1 covers the two demo paths:
//! - GCP→AWS (per terrashift_plan.md §17 demo): `aws_vpc`, `aws_subnet`,
//!   `aws_security_group`, `aws_instance`, `aws_s3_bucket`.
//! - AWS→Azure (per pre-flight Decision 10 — PratikMahajan fixture):
//!   `azurerm_virtual_network`, `azurerm_subnet`,
//!   `azurerm_network_security_group`, `azurerm_linux_virtual_machine`,
//!   `azurerm_storage_account`.
//!
//! Stage 5 (P-27) adds dynamic blocks, count, for_each, and externalises
//! the registry to the `terrashift-mappings` repo.

use crate::generator::errors::GeneratorError;
use crate::mapper::{AttributeValue, MappedResource};
use hcl::expr::{Traversal, TraversalOperator};
use hcl::{Block, Body, Expression, Identifier, Variable};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use terrashift_knowledge::ProviderSchema;

/// One template — pure function over a single mapped resource.
type TemplateFn = fn(&MappedResource) -> Result<Block, GeneratorError>;

/// Lookup table keyed by `target_type` (e.g. `"aws_vpc"`).
///
/// Two-tier emission strategy (Article I — deterministic, Article XIII rule 8 —
/// no hardcoding):
///
/// 1. **Hand-curated templates** (`by_type`) — pure-fn templates for resource
///    types whose HCL emission needs custom shaping beyond flat-attribute
///    pass-through (e.g., `aws_s3_bucket` uses `bucket` not `name` for the
///    primary identifier; nested-block resources are stage 5+).
///
/// 2. **Schema-derived fallback** (`schema_cache`) — populated from the seed
///    JSONs at construction. Every resource type present in any loaded
///    `ProviderSchema` becomes emittable: required attributes are derived
///    from `attribute_type.required == true`; optional attributes from
///    `optional == true && !computed`. Hand-curated takes precedence —
///    schema_cache entries are never inserted for keys already in by_type.
///
/// The fallback is the production-grade path: with 3 cloud providers'
/// full schemas captured under `libs/knowledge/seed/`, the registry covers
/// **every** resource type those providers expose without any source-level
/// enumeration of resource type names (Article XIII rule 8 — no hardcoding).
pub struct TemplateRegistry {
    by_type: HashMap<&'static str, TemplateFn>,
    /// Schema-derived (required, optional) attribute lists, keyed by target
    /// resource type. Owned `String`s because schema-derived types are
    /// discovered at runtime, not compile time. BTreeMap for deterministic
    /// iteration order under Article VI.
    schema_cache: BTreeMap<String, SchemaTemplate>,
}

/// One entry in the schema-driven fallback table.
struct SchemaTemplate {
    required: Vec<String>,
    optional: Vec<String>,
}

impl TemplateRegistry {
    /// Build the Stage 1 registry. All templates registered up front; missing
    /// types return `Err(TemplateMiss)` at call time.
    pub fn stage1() -> Self {
        let mut by_type: HashMap<&'static str, TemplateFn> = HashMap::new();

        // GCP→AWS demo path.
        by_type.insert("aws_vpc", template_aws_vpc);
        by_type.insert("aws_subnet", template_aws_subnet);
        by_type.insert("aws_security_group", template_aws_security_group);
        by_type.insert("aws_instance", template_aws_instance);
        by_type.insert("aws_s3_bucket", template_aws_s3_bucket);

        // AWS→Azure (Pratik fixture) path.
        by_type.insert("azurerm_virtual_network", template_azurerm_virtual_network);
        by_type.insert("azurerm_subnet", template_azurerm_subnet);
        by_type.insert(
            "azurerm_network_security_group",
            template_azurerm_network_security_group,
        );
        by_type.insert(
            "azurerm_linux_virtual_machine",
            template_azurerm_linux_virtual_machine,
        );
        by_type.insert("azurerm_storage_account", template_azurerm_storage_account);

        // S14 — Stage 2 patterns.
        by_type.insert("aws_iam_role", template_aws_iam_role);
        by_type.insert("google_storage_bucket", template_google_storage_bucket);

        // GCP target parity with AWS + Azure (Stage 3 / S16 + S20 prep).
        // Covers the canonical 5-resource networking-and-compute set so
        // aws→google and azurerm→google migrations have the same template
        // surface area as aws→azurerm. Nested-block emission (boot_disk,
        // network_interface, allow/deny rules) is Stage 5+ work; flat
        // attributes ship now.
        by_type.insert("google_compute_network", template_google_compute_network);
        by_type.insert(
            "google_compute_subnetwork",
            template_google_compute_subnetwork,
        );
        by_type.insert("google_compute_firewall", template_google_compute_firewall);
        by_type.insert("google_compute_instance", template_google_compute_instance);

        // Pratik-fixture E2E expansion — additional azurerm types the
        // Mapper proposes when migrating a real-world AWS VPC module.
        by_type.insert("azurerm_route_table", template_azurerm_route_table);
        by_type.insert("azurerm_public_ip", template_azurerm_public_ip);
        by_type.insert(
            "azurerm_subnet_network_security_group_association",
            template_azurerm_subnet_nsg_association,
        );
        by_type.insert(
            "azurerm_subnet_route_table_association",
            template_azurerm_subnet_route_table_association,
        );
        by_type.insert("azurerm_resource_group", template_azurerm_resource_group);

        Self {
            by_type,
            schema_cache: BTreeMap::new(),
        }
    }

    /// Production constructor — Stage 1 hand-curated templates PLUS a
    /// schema-derived fallback for every resource in the provided schemas.
    ///
    /// Hand-curated templates win over schema-derived for the same target
    /// type (so the special `aws_s3_bucket → bucket` shape stays correct).
    /// All other types — thousands across the 3 cloud providers — become
    /// emittable through the generic flat-attribute pass-through.
    ///
    /// Per Article XIII rule 8 (no hardcoding) and the project memory rule
    /// "seed/ is schema truth", this is how new resource types are added:
    /// drop a new schema into `libs/knowledge/seed/<provider>/<version>/`,
    /// rebuild, and every resource becomes emittable without source-level
    /// enumeration.
    pub fn with_schemas(schemas: impl IntoIterator<Item = Arc<ProviderSchema>>) -> Self {
        let mut me = Self::stage1();
        let by_type_keys: std::collections::HashSet<&'static str> =
            me.by_type.keys().copied().collect();

        for schema in schemas {
            for (target_type, resource_schema) in &schema.resources {
                // Skip if a hand-curated template already covers this type —
                // explicit overrides win over schema-derived defaults.
                if by_type_keys.contains(target_type.as_str()) {
                    continue;
                }

                // Filter attributes: required → required list; optional &&
                // !computed → optional list; computed-only → skip (Terraform
                // computes them post-apply, operator can't set them).
                let required: Vec<String> = resource_schema
                    .attributes
                    .iter()
                    .filter(|(_, a)| a.required)
                    .map(|(name, _)| name.clone())
                    .collect();
                let optional: Vec<String> = resource_schema
                    .attributes
                    .iter()
                    .filter(|(_, a)| a.optional && !a.computed)
                    .map(|(name, _)| name.clone())
                    .collect();

                me.schema_cache
                    .insert(target_type.clone(), SchemaTemplate { required, optional });
            }
        }

        tracing::info!(
            hand_curated = me.by_type.len(),
            schema_derived = me.schema_cache.len(),
            "TemplateRegistry initialized"
        );
        me
    }

    /// Look up the template fn for a `target_type`. None means template-miss.
    /// Kept for backwards-compatibility with callers that only need the
    /// hand-curated layer (e.g., tests asserting Stage 1 surface).
    pub fn template_for(&self, target_type: &str) -> Option<TemplateFn> {
        self.by_type.get(target_type).copied()
    }

    /// Emit one `MappedResource` to an HCL `Block` — the production entry
    /// point. Dispatches in order:
    ///   1. Hand-curated template if registered.
    ///   2. Schema-derived flat emission if the type is in any loaded schema.
    ///   3. `Err(TemplateMiss)` otherwise (Article IV — loud failure).
    pub fn emit(&self, r: &MappedResource) -> Result<Block, GeneratorError> {
        if let Some(template) = self.by_type.get(r.target_type.as_str()) {
            return template(r);
        }
        if let Some(SchemaTemplate { required, optional }) = self.schema_cache.get(&r.target_type) {
            let req_refs: Vec<&str> = required.iter().map(|s| s.as_str()).collect();
            let opt_refs: Vec<&str> = optional.iter().map(|s| s.as_str()).collect();
            return build_resource_block(r, &req_refs, &opt_refs);
        }
        Err(GeneratorError::TemplateMiss {
            target_type: r.target_type.clone(),
        })
    }

    /// Iterate every emittable `target_type` — hand-curated and schema-derived
    /// — for diagnostic and Mapper-prompt use. Schema-derived types are owned
    /// `String`s so the API returns `&str`.
    pub fn registered_types(&self) -> impl Iterator<Item = &str> + '_ {
        self.by_type
            .keys()
            .copied()
            .chain(self.schema_cache.keys().map(|s| s.as_str()))
    }

    /// How many templates are registered, total (hand-curated + schema). Used by tests.
    pub fn len(&self) -> usize {
        self.by_type.len() + self.schema_cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty() && self.schema_cache.is_empty()
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::stage1()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers — convert AttributeValue → hcl::Expression
// ─────────────────────────────────────────────────────────────────────────

/// Convert one `AttributeValue` to an `hcl::Expression`. References emit
/// unquoted (raw HCL), strings emit quoted, numbers/bools native.
fn to_expression(value: &AttributeValue) -> Result<Expression, GeneratorError> {
    Ok(match value {
        AttributeValue::String(s) => Expression::from(s.as_str()),
        AttributeValue::Number(n) => Expression::from(*n),
        AttributeValue::Bool(b) => Expression::from(*b),
        AttributeValue::Reference(r) => parse_reference(r)?,
        AttributeValue::List(items) => {
            let exprs: Result<Vec<_>, _> = items.iter().map(to_expression).collect();
            Expression::from_iter(exprs?)
        }
        AttributeValue::Map(map) => map_to_object_expression(map)?,
    })
}

/// Parse a dotted reference path (`aws_vpc.main.id`, `var.region`,
/// `data.aws_ami.example.id`) into an HCL `Expression::Variable` (single
/// segment) or `Expression::Traversal` (multi-segment).
///
/// hcl-rs 0.18 doesn't impl `FromStr` for `Expression`, so we build the
/// traversal manually. `Identifier::new` rejects invalid HCL identifiers
/// — for our internal references the Mapper is contracted to emit valid
/// identifiers, so anything else is a Mapper bug surfaced via
/// `GeneratorError::Hcl`.
///
/// **Empty input is a hard error** (Article IV — surfacing the upstream
/// Mapper bug rather than emitting `attr = ""`).
fn parse_reference(s: &str) -> Result<Expression, GeneratorError> {
    if s.is_empty() {
        return Err(GeneratorError::EmptyReference);
    }
    let mut parts = s.split('.');
    // We just checked `s.is_empty()`, so `split` always yields at least
    // one segment. Use the same pattern as backup.rs:62 — `unwrap_or` is
    // `Option::unwrap_or` (deliberate default), allowed by clippy because
    // it's not a panic shortcut.
    let first = parts.next().unwrap_or("");
    if first.is_empty() {
        // Reference like ".foo.bar" — leading dot. Same Article IV concern.
        return Err(GeneratorError::EmptyReference);
    }
    let head = Variable::new(first).map_err(GeneratorError::Hcl)?;
    let operators: Vec<TraversalOperator> = parts
        .map(|p| {
            Identifier::new(p)
                .map(TraversalOperator::GetAttr)
                .map_err(GeneratorError::Hcl)
        })
        .collect::<Result<_, _>>()?;
    if operators.is_empty() {
        Ok(Expression::Variable(head))
    } else {
        Ok(Expression::Traversal(Box::new(Traversal {
            expr: Expression::Variable(head),
            operators,
        })))
    }
}

/// Convert a `BTreeMap<String, AttributeValue>` to an HCL object expression
/// (for things like `tags = { Name = "main" }`). BTreeMap iteration order
/// is sorted — that's the determinism guarantee per Article VI.
fn map_to_object_expression(
    map: &BTreeMap<String, AttributeValue>,
) -> Result<Expression, GeneratorError> {
    let mut object: hcl::expr::Object<hcl::expr::ObjectKey, Expression> = hcl::expr::Object::new();
    for (k, v) in map {
        object.insert(
            hcl::expr::ObjectKey::Identifier(hcl::Identifier::sanitized(k)),
            to_expression(v)?,
        );
    }
    Ok(Expression::Object(object))
}

/// Build a resource block of `target_type "name"` with the given attribute
/// keys taken from `r.attributes`. `required_keys` must be *present* (any
/// `AttributeValue` type — Mapper output legitimately includes
/// `Reference("azurerm_resource_group.main.name")` for required slots like
/// `resource_group_name`). Missing returns `Err`. Optional keys are
/// appended only when present.
fn build_resource_block(
    r: &MappedResource,
    required_keys: &[&str],
    optional_keys: &[&str],
) -> Result<Block, GeneratorError> {
    let mut builder = Block::builder("resource")
        .add_label(r.target_type.clone())
        .add_label(r.target_name.clone());

    for k in required_keys {
        let value =
            r.attributes
                .get(*k)
                .ok_or_else(|| crate::mapper::MapperLookupError::Missing {
                    addr: r.target_addr.clone(),
                    attr: (*k).to_string(),
                })?;
        builder = builder.add_attribute((*k, to_expression(value)?));
    }

    for k in optional_keys {
        if let Some(value) = r.attributes.get(*k) {
            builder = builder.add_attribute((*k, to_expression(value)?));
        }
    }

    // Tags (and similar nested objects) — emit if present and Map-shaped.
    if let Some(tags) = r.optional_map("tags") {
        builder = builder.add_attribute(("tags", map_to_object_expression(tags)?));
    }

    Ok(builder.build())
}

// ─────────────────────────────────────────────────────────────────────────
// AWS templates (Stage 1 GCP→AWS demo path)
// ─────────────────────────────────────────────────────────────────────────

fn template_aws_vpc(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["cidr_block"],
        &["instance_tenancy", "enable_dns_support"],
    )
}

fn template_aws_subnet(r: &MappedResource) -> Result<Block, GeneratorError> {
    // vpc_id is typically a Reference (e.g., aws_vpc.main.id), not a
    // literal string — so it goes through optional_passthrough.
    build_resource_block(
        r,
        &["cidr_block"],
        &["vpc_id", "availability_zone", "map_public_ip_on_launch"],
    )
}

fn template_aws_security_group(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(r, &["name"], &["vpc_id", "description"])
}

fn template_aws_instance(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["instance_type"],
        &[
            "ami",
            "subnet_id",
            "key_name",
            "associate_public_ip_address",
        ],
    )
}

fn template_aws_s3_bucket(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(r, &["bucket"], &["acl", "force_destroy"])
}

// ─────────────────────────────────────────────────────────────────────────
// Azure templates (AWS→Azure Pratik fixture path)
// ─────────────────────────────────────────────────────────────────────────

fn template_azurerm_virtual_network(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "resource_group_name", "location"],
        &["address_space"],
    )
}

fn template_azurerm_subnet(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "resource_group_name", "virtual_network_name"],
        &["address_prefixes"],
    )
}

fn template_azurerm_network_security_group(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(r, &["name", "location", "resource_group_name"], &[])
}

fn template_azurerm_linux_virtual_machine(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "resource_group_name", "location", "size"],
        &["admin_username", "network_interface_ids"],
    )
}

fn template_azurerm_storage_account(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &[
            "name",
            "resource_group_name",
            "location",
            "account_tier",
            "account_replication_type",
        ],
        &[],
    )
}

// ─────────────────────────────────────────────────────────────────────────
// S14 — Stage 2 templates
// ─────────────────────────────────────────────────────────────────────────

/// AWS IAM role. `assume_role_policy` is typically a JSON-encoded string
/// (HCL idiomatic when the policy is small; large policies use
/// `data "aws_iam_policy_document"` which is a Stage 5+ refinement).
fn template_aws_iam_role(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["assume_role_policy"],
        &["name", "description", "path", "permissions_boundary"],
    )
}

/// GCP storage bucket. Stage 2 covers the flat-attribute path; nested
/// `lifecycle_rule` blocks land in Stage 5+ (P-27 dynamic blocks).
fn template_google_storage_bucket(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "location"],
        &[
            "force_destroy",
            "storage_class",
            "uniform_bucket_level_access",
            "versioning",
        ],
    )
}

/// GCP VPC — analog of `aws_vpc` and `azurerm_virtual_network`. Stage 1
/// emits flat attributes; subnetworks are emitted as siblings via
/// `template_google_compute_subnetwork`. Stage 5+ adds nested
/// `subnet { … }` block emission for the `auto_create_subnetworks =
/// false` + inline subnetwork pattern.
fn template_google_compute_network(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name"],
        &[
            "description",
            "auto_create_subnetworks",
            "routing_mode",
            "mtu",
            "delete_default_routes_on_create",
        ],
    )
}

/// GCP subnet — analog of `aws_subnet` and `azurerm_subnet`. The required
/// CIDR is `ip_cidr_range` (vs AWS `cidr_block`, vs Azure `address_prefixes`).
fn template_google_compute_subnetwork(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "ip_cidr_range", "network", "region"],
        &[
            "description",
            "private_ip_google_access",
            "purpose",
            "role",
            "stack_type",
            "ipv6_access_type",
        ],
    )
}

/// GCP firewall — analog of `aws_security_group` and `azurerm_network_security_group`.
/// Stage 1 emits flat attributes only; the nested `allow {}` / `deny {}`
/// blocks are deferred to Stage 5+ (dynamic blocks). Operators add rules
/// via sibling `google_compute_firewall_rule` resources or the inline
/// blocks once nested-block emission ships.
fn template_google_compute_firewall(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "network"],
        &[
            "description",
            "direction",
            "priority",
            "source_ranges",
            "destination_ranges",
            "source_tags",
            "target_tags",
            "disabled",
            "log_config",
        ],
    )
}

/// GCP compute instance — analog of `aws_instance` and `azurerm_linux_virtual_machine`.
/// Stage 1 emits flat attributes; the required `boot_disk { initialize_params { … } }`
/// and `network_interface { … }` nested blocks are deferred to Stage 5+
/// (dynamic blocks). Recovery agent (S10) typically gets the operator past
/// the resulting `terraform validate` errors via `set_attribute` on the
/// nested-block-as-flat-string fallback shape.
fn template_google_compute_instance(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "machine_type", "zone"],
        &[
            "description",
            "tags",
            "labels",
            "metadata_startup_script",
            "can_ip_forward",
            "deletion_protection",
            "hostname",
        ],
    )
}

/// Azure route table. The nested `route` blocks themselves are deferred
/// to Stage 5+ (dynamic blocks); Stage 2 emits the table shell + flat
/// attributes. Operators add routes via `azurerm_route` siblings.
fn template_azurerm_route_table(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &["name", "location", "resource_group_name"],
        &["disable_bgp_route_propagation"],
    )
}

/// Azure public IP — typical mapping target when an `aws_internet_gateway`
/// gets translated (Azure has no IGW concept; public IP is the closest
/// concrete equivalent).
fn template_azurerm_public_ip(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(
        r,
        &[
            "name",
            "location",
            "resource_group_name",
            "allocation_method",
        ],
        &["sku", "domain_name_label", "ip_version"],
    )
}

/// Azure subnet ↔ NSG association — what `aws_route_table_association`
/// often gets mapped to (semantically debatable; Recovery agent typically
/// fixes it to `azurerm_subnet_route_table_association`, but we register
/// the template so Generator can emit either path without TemplateMiss).
fn template_azurerm_subnet_nsg_association(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(r, &["subnet_id", "network_security_group_id"], &[])
}

/// Azure subnet ↔ route table association — the semantically-correct
/// target type for `aws_route_table_association` migrations.
fn template_azurerm_subnet_route_table_association(
    r: &MappedResource,
) -> Result<Block, GeneratorError> {
    build_resource_block(r, &["subnet_id", "route_table_id"], &[])
}

/// Azure resource group — top-level container that most other azurerm
/// resources reference via `resource_group_name`.
fn template_azurerm_resource_group(r: &MappedResource) -> Result<Block, GeneratorError> {
    build_resource_block(r, &["name", "location"], &[])
}

/// Sort `MappedResource`s by `target_addr` for deterministic emission order
/// (Article VI). Caller passes a slice; we return a sorted Vec of refs.
pub fn sort_resources(resources: &[MappedResource]) -> Vec<&MappedResource> {
    let mut sorted: Vec<&MappedResource> = resources.iter().collect();
    sorted.sort_by(|a, b| a.target_addr.cmp(&b.target_addr));
    sorted
}

/// Group resources by `target_type` so each group writes to its own file
/// (`aws_vpc.tf`, `aws_subnet.tf`, ...). Within a group, resources retain
/// the sort-by-target_addr order from `sort_resources`.
pub fn group_by_target_type<'a>(
    resources: &'a [&'a MappedResource],
) -> BTreeMap<String, Vec<&'a MappedResource>> {
    let mut groups: BTreeMap<String, Vec<&MappedResource>> = BTreeMap::new();
    for r in resources {
        groups.entry(r.target_type.clone()).or_default().push(*r);
    }
    groups
}

/// Compose a `Body` from a slice of `Block`s. Stable iteration order.
pub fn body_from_blocks(blocks: Vec<Block>) -> Body {
    let mut body = Body::builder();
    for block in blocks {
        body = body.add_block(block);
    }
    body.build()
}
