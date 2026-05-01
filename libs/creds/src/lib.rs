//! Credential broker — Article V enforcement point.
//!
//! Pattern: stakpak_arch.md section 27 (secret detection / redaction),
//! terrashift_diagrams.html LLD-4 (credential flow with 9 numbered steps).
//!
//! Constitution: Article V (heart of it — no long-lived credentials in
//! process memory; LLM never sees raw secrets; every credential operation
//! audited). Article XIII rules 5, 7, 10.
//!
//! Two credential classes are protected:
//! 1. Cloud provider credentials (AWS STS, GCP ADC, Azure managed identity)
//! 2. LLM provider keys (Anthropic, OpenAI, custom Vodafone gateway)
//!
//! The LLM never sees raw secrets — only `{{secret:name}}` references
//! resolved at the tool-execution boundary.
//!
//! Modules to be filled in by P-NN prompts (P-10):
//! - `broker.rs` — CredentialBroker async trait
//! - `aws.rs` — AwsBroker (STS AssumeRole)
//! - `gcp.rs` — GcpBroker (ADC + workload identity federation)
//! - `azure.rs` — AzureBroker (managed identity)
//! - `secret_substitution.rs` — `{{secret:...}}` resolution
//! - `scrubber.rs` — gitleaks rule set + entropy filter (per section 27)
//! - `zeroize_wrapper.rs` — Zeroizing<Credential> wrapper

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
