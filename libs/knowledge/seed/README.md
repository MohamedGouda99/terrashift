# Knowledge seed bundle

Pre-built provider schemas that the release binary embeds via
`include_bytes!` and `KnowledgeService::seed_from_bundle` loads at startup,
so the cache is populated on first install rather than empty. Avoids
cold-start latency hitting `terraform init` + `terraform providers schema -json`
on the first migration run.

## Layout (post RFC schema-source-migration)

One full `ProviderSchema` per `(provider, version)` pair, encoded as a
single JSON file. Versions live as concrete directory names — no `latest`,
no `terrashift-seed-XXXX` placeholder. Article VI compliant by construction.

```
seed/
├── manifest.toml                ← build-time pins (this file is the input
│                                  to `cargo xtask capture-schemas`)
├── aws/
│   └── 5.30.0/
│       └── schema.json
├── azurerm/
│   └── 3.110.0/
│       └── schema.json
└── google/
    └── 5.40.2/
        └── schema.json
```

- **Top-level dirs** = provider keys (`aws`, `azurerm`, `google`). The
  loader infers provider from this directory name.
- **Second-level dirs** = concrete versions (`5.30.0`). The loader infers
  version from this directory name.
- **`schema.json` files** = one complete `ProviderSchema` each. Includes
  resources, data_sources, fetched_at.

## Per-file shape

Each `schema.json` matches the public `ProviderSchema` struct
(`libs/knowledge/src/types.rs`). The shape is what
`terraform providers schema -json` emits, normalised through
`TerraformCliSchemaFetcher::fetch`.

```json
{
  "provider": "aws",
  "version": "5.30.0",
  "resources": {
    "aws_vpc": {
      "name": "aws_vpc",
      "description": "Provides a VPC resource.",
      "attributes": {
        "cidr_block": {
          "name": "cidr_block",
          "attribute_type": "string",
          "required": true,
          "optional": false,
          "computed": false,
          "sensitive": false,
          "deprecated": null,
          "description": "..."
        }
      }
    }
  },
  "data_sources": { },
  "fetched_at": "2026-05-07T11:00:00Z"
}
```

## Refreshing this bundle

The seed is produced by `cargo xtask capture-schemas` from the pins in
`manifest.toml`. Workflow:

```bash
# Edit pins.
$EDITOR libs/knowledge/seed/manifest.toml

# Capture schemas via the local terraform CLI.
cargo xtask capture-schemas

# Verify nothing else broke.
cargo nextest run -p terrashift-knowledge
```

The capture step writes `schema.json` files into the layout above; commit
the result alongside the manifest change. Article VI: every commit that
touches this bundle pins a concrete, named version — no `latest` allowed.

## Refreshing against the live registry at runtime

Operators don't need the bundle — they can pull schemas on demand from
the shell:

```bash
terrashift schema update --provider aws --version 5.30.0
terrashift schema list
```

Updates land in `~/.terrashift/schemas/`, indexed by
`~/.terrashift/schemas/manifest.json`, and emit one
`AuditPayload::SchemaCapture` entry per (provider, version) per
invocation (Article V).

## Constitution

- **Article V** — every capture is recorded in the audit log; the operator,
  resolved version, and Terraform version are all preserved.
- **Article VI** — directory names ARE the version pin; there's no path
  through this loader that resolves `latest`.
- **Article IX** — schemas are NOT user data (they are reproducibly
  re-fetchable from the same Terraform CLI subprocess). `gc` is therefore
  permitted on this directory shape.
