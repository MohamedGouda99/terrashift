//! Executor — sandboxed `terraform apply` with command-level approval.
//! Pattern: terrashift_plan.md §5; arrives in P-09.
//! Article XIII rule 6 (never approve `terraform apply` wholesale — tree-sitter
//! parses each command in the pipeline and applies max-restrictive policy).
