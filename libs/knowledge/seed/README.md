# Knowledge seed bundle

Pre-built `ProviderSchema` JSON snapshots that `LocalSchemaStore::seed_from_bundle()`
loads at startup, so the cache is **populated on first install** rather than
empty. Avoids cold-start latency hitting `registry.terraform.io` for every
provider/version pair.

## Source

Each JSON file maps to a Terrashift `ProviderSchema` (`libs/knowledge/src/types.rs`).
The bundled snapshots are derived from
[CloudForge resource-catalog](https://github.com/MohamedGouda99/CloudForge)
TypeScript files at `shared/resource-catalog/src/{aws,azure,gcp}/`.

The `version` field on each snapshot is `cloudforge-YYYY.MM` (not a Terraform
provider version) because these schemas are CloudForge's curated subset, not a
1:1 dump of the full Terraform Registry response. To pin against a real
Terraform provider version (e.g., `aws=5.30.0`), run:

```sh
terrashift schemas refresh --provider aws --version 5.30.0
```

…which will fetch from the live registry and supersede this seed for that
specific (provider, version) tuple.

## Files

| File | Provider | Resources |
|---|---|---|
| `aws-cloudforge.json` | `aws` | 5 networking primitives (vpc, subnet, internet_gateway, route_table, route_table_association) |
| `azurerm-cloudforge.json` | `azurerm` | 6 resources (virtual_network, subnet, resource_group, network_security_group, route_table, public_ip) |

Expand by running the converter (see `scripts/import-cloudforge.mjs`).

## Schema shape

```json
{
  "provider": "aws",
  "version": "cloudforge-2025.01",
  "resources": {
    "<resource_type>": {
      "name": "<resource_type>",
      "description": "...",
      "attributes": {
        "<attr>": {
          "name": "<attr>",
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
  },
  "data_sources": {},
  "fetched_at": "2025-01-01T00:00:00Z"
}
```
