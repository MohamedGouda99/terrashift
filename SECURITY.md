# Security policy

Terrashift handles cloud credentials, runs Terraform plans, and writes audit
logs that may contain sensitive metadata. We take security seriously.

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Use [GitHub's private security advisory flow][advisory] to report
vulnerabilities. Reports are triaged within 5 business days.

[advisory]: https://github.com/MohamedGouda99/terrashift/security/advisories/new

For urgent or out-of-band reports, email the repo owner via the address listed
on the GitHub profile.

## What's in scope

- The `terrashift` binary, all libraries under `libs/`, and the `cli/` and
  `tui/` crates.
- The CI/release pipelines under `.github/workflows/`.
- Default profiles and fixtures bundled with the release artifacts.

## What's out of scope

- Vulnerabilities in upstream crates we depend on (`stakai`, `rmcp`,
  `lancedb`, `hcl-rs`, `sqlx`, `tree-sitter*`, etc.) — please report
  those upstream.
- Vulnerabilities that require an attacker who already has credential
  access to the operator's machine (Terrashift's threat model assumes
  the operator is trusted).
- Misuse via deliberately broken profiles where the operator has
  disabled audit-chain verification or shell-command approval gates
  (those are documented kill-switches, not vulnerabilities).

## Disclosure

We coordinate disclosure timelines with reporters. Default is 90 days from
acknowledgement to public disclosure, accelerated when there's evidence of
active exploitation.

## Secret handling

Per [Constitution Article V](docs/governance/CONSTITUTION.md#article-v):

- API keys and cloud creds are read from environment variables at request
  time and never persisted to disk or logs.
- All secret material is wrapped in `Zeroizing<String>` and zeroed on drop.
- Audit-log scrubbing runs a gitleaks regex set + entropy filter before
  every write; raw secret detection is a panic, not a soft-fail
  (Article XIII rule 5).

If you find a path that bypasses these, that's an in-scope vulnerability —
please report it.
