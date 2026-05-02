# Terrashift golden migration suite

Hand-curated source/plan/expected fixtures driving the eval framework
(`libs/eval`). Each subdirectory is one migration whose Generator output
must stay byte-stable across runs (Article VI — "same migration today
gives same output six months from now").

## Convention — files per fixture

```
NNN_short_descriptive_name/
├── manifest.toml         # GoldenManifest — name, providers, articles
├── source.tf             # source-cloud HCL (informational in S3b;
│                         # consumed by Mapper in S4)
├── mapping_plan.json     # pre-curated MappingPlan (Mapper output emulation
│                         # until S4 wires the real Mapper)
└── expected/
    └── <target_type>.tf  # one .tf per target_type group; what the
                          # Generator must emit byte-for-byte
```

## How to add a new golden

1. Pick a numeric prefix (`004_`, `005_`, …) and a short slug.
2. Author `manifest.toml`, `source.tf`, `mapping_plan.json` by hand.
3. Run the bootstrap command to capture the Generator's actual output
   into `expected/`:

   ```powershell
   $env:BOOTSTRAP_GOLDENS=1; cargo test -p terrashift-eval bootstrap_goldens; Remove-Item Env:BOOTSTRAP_GOLDENS
   ```

4. Inspect the contents of `expected/`. They should look like the HCL
   you intended. If wrong, adjust `mapping_plan.json` and re-bootstrap.
5. Once `expected/` is correct, commit the whole directory.
6. From now on, the eval framework asserts `expected/` byte-equality on
   every PR; any drift is a regression.

## Why first-party (Apache-2)

S3b ships these as part of the workspace. They're tiny (~30-60 LOC each)
and validate Stage 1 demo paths. SESSION_PLAN row 7 (`Eval expansion`)
moves the larger golden set to a private `terrashift-evals` repo when we
cross 10 fixtures and they include customer-derived shapes.

The GPL-2 PratikMahajan fixture (per pre-flight Decision 10) lives at
`fixtures/aws-to-azure-real/` and is **gitignored** — never bundled.

## Constitution

- **Article III** — these fixtures are the source of truth for AI safety
  (Validator/Mapper/Generator regression detection).
- **Article VI** — byte-stable emit makes the comparison meaningful.
- **Article XII rule 4** — the framework's regression-gate plumbing
  funnels through these fixtures.
