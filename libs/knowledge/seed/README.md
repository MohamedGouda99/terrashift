# Knowledge seed bundle

Pre-built per-resource schema JSON files that
`KnowledgeService::seed_from_bundle()` loads at startup, so the cache is
**populated on first install** rather than empty. Avoids cold-start
latency hitting `registry.terraform.io` for every provider/version pair
on the first migration run.

## Layout

```
seed/
├── aws/
│   ├── networking/
│   │   ├── aws_vpc.json
│   │   ├── aws_subnet.json
│   │   └── ...
│   ├── compute/
│   ├── storage/
│   └── ...
├── azurerm/
│   ├── networking/
│   ├── security/
│   ├── storage/
│   └── ...
├── google/
│   ├── networking/
│   ├── compute/
│   └── ...
└── scripts/
    └── import-resource-catalog.mjs
```

- **Top-level dirs** = provider keys (`aws`, `azurerm`, `google`). The
  loader infers provider from this directory name.
- **Subdirectories** = categories (free-form; the loader walks
  recursively so any depth works).
- **Leaf `.json` files** = one `ResourceSchema` each.

## Per-file shape

Each `<resource_type>.json` matches the
`terrashift_knowledge::ResourceSchema` struct
(`libs/knowledge/src/types.rs`):

```json
{
  "name": "aws_vpc",
  "description": "Virtual Private Cloud — isolated virtual network in AWS...",
  "attributes": {
    "<attr_name>": {
      "name": "<attr_name>",
      "attribute_type": "string|number|bool|list(string)|map(string)|...",
      "required": true|false,
      "optional": true|false,
      "computed": false,
      "sensitive": false,
      "deprecated": null,
      "description": "..."
    }
  }
}
```

The loader infers `provider` from the parent directory
(`aws/networking/aws_vpc.json` → provider = `"aws"`) and stamps the
`version` field with `terrashift-seed-2025.01` (a label distinct from
real Terraform provider versions).

## Bulk-converting an external resource catalog

If you have a TypeScript-based resource catalog (e.g.,
[CloudForge](https://github.com/MohamedGouda99/CloudForge) or
similar), the included Node converter walks the `.ts` files and emits
JSON in the layout above.

```sh
# One-time setup: install Node 18+ if not already present
node --version

# Convert your external catalog into the seed
node libs/knowledge/seed/scripts/import-resource-catalog.mjs \
  --src "C:/path/to/your/resource-catalog/src" \
  --dst libs/knowledge/seed
```

The converter expects each `.ts` file to export a `ServiceDefinition`
object with `terraform_resource`, `description`, and
`inputs.{required,optional}` fields. See the converter source for the
exact shape it accepts; adapt as needed for your catalog.

## Refreshing against the live registry

The seed is a starting point, not a substitute for live Terraform
provider schemas. To pin against a real provider version:

```sh
terrashift schemas refresh --provider aws --version 5.30.0
```

(This CLI command lands as part of the S17a follow-up — for now it's a
placeholder; the underlying `KnowledgeService::sync_provider()` is
already wired and operator-callable from a small driver binary.)

## What ships in this initial bundle

Curated minimal subset to support the AWS→Azure VPC migration demo
(see `libs/engine/tests/pratik_e2e_test.rs`):

| Provider | Resources |
|---|---|
| `aws` | `aws_vpc`, `aws_subnet`, `aws_internet_gateway`, `aws_route_table`, `aws_route_table_association` |
| `azurerm` | `azurerm_virtual_network`, `azurerm_subnet`, `azurerm_resource_group`, `azurerm_route_table`, `azurerm_public_ip`, `azurerm_network_security_group`, `azurerm_subnet_route_table_association` |

Run the bulk converter on your full external catalog to expand from
this minimal seed to comprehensive coverage.
