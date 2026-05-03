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

/// One template — pure function over a single mapped resource.
type TemplateFn = fn(&MappedResource) -> Result<Block, GeneratorError>;

/// Lookup table keyed by `target_type` (e.g. `"aws_vpc"`).
pub struct TemplateRegistry {
    by_type: HashMap<&'static str, TemplateFn>,
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

        Self { by_type }
    }

    /// Look up the template fn for a `target_type`. None means template-miss.
    pub fn template_for(&self, target_type: &str) -> Option<TemplateFn> {
        self.by_type.get(target_type).copied()
    }

    /// How many templates are registered. Used by tests.
    pub fn len(&self) -> usize {
        self.by_type.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
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
