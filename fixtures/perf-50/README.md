# 50-resource AWS perf fixture

50 source resources across 6 types — used by `libs/engine/benches/pipeline.rs`
to measure Scanner / Generator / Validator wall time and memory at MVP-strict
scale.

| Type | Count |
|---|---|
| `aws_vpc` | 1 |
| `aws_subnet` | 10 |
| `aws_security_group` | 10 |
| `aws_instance` | 10 |
| `aws_s3_bucket` | 10 |
| `aws_iam_role` | 9 |
| **Total** | **50** |

The 9 IAM roles are **expected to skip** during aws→azurerm migration —
Stage 1 has no `azurerm_role_definition` template registered. Best-case emit
ratio is therefore 41/50 = 82%, consistent with the gate-criterion-#1
threshold of 80%.

This fixture is **bench-only**. It is not part of any e2e test gate.
