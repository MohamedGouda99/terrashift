//! The 9-component migration pipeline.
//!
//! Pattern: terrashift_plan.md §5 (agentic-vs-deterministic split),
//! terrashift_diagrams.html HLD-2 (pipeline) + LLD-2 (lifecycle).
//!
//! Constitution: Article I (only 3 components are agents — Recovery, Cost
//! Optimizer, Cutover. The other 6 are deterministic or LLM-with-structured-
//! output. Adding a new agent requires explicit RFC + stage-gate approval).
//! Article III (Validator must be on the path between Mapper and Generator).
//! Article XIII rule 1 (don't bypass message conversion pipeline).
//!
//! Stage 1 scope:
//! - Scanner, Mapper (single LLM call), Planner (single LLM call), Generator,
//!   Validator, Executor, Verifier
//! - NO agents in Stage 1. Recovery + Cost Optimizer arrive in Stage 2.
//!
//! Modules:
//! - `scanner/`        — deterministic HCL parser (P-04)
//! - `mapper/`         — LLM with structured output (P-05)
//! - `planner/`        — LLM with structured output (TBD prompt)
//! - `generator/`      — deterministic templates + hcl-rs + .backup/ (P-08)
//! - `validator/`      — Article III gate (P-06)
//! - `executor/`       — sandboxed terraform apply (P-09)
//! - `verifier/`       — post-apply state diff (TBD prompt)
//! - `recovery/`       — true agent, Stage 2
//! - `cost_optimizer/` — true agent, Stage 2

pub mod cost_optimizer;
pub mod executor;
pub mod generator;
pub mod mapper;
pub mod planner;
pub mod recovery;
pub mod scanner;
pub mod validator;
pub mod verifier;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
