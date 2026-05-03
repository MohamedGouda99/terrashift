//! Terrashift credential broker — Article V cornerstone.
//!
//! Pattern: terrashift_plan.md §8 (auth & credentials),
//! `Terrashift_Plan.docx §8` longform; stakpak_arch.md §27 (secret
//! detection / redaction); refs/stakpak/libs/shared/src/secrets/
//! (broker pattern + zeroize discipline).
//!
//! Constitution: Article V (LLM never sees raw values; broker resolves
//! at the tool-execution boundary; every operation audit-logged with
//! reference, not value), Article XIII rule 5 (`pre_llm_check` is the
//! outbound proactive layer; audit-writer panic is the inbound runtime
//! layer), Article XIII rule 7 (no disk-bound secret writes — Stage 1
//! is in-memory only), Article XIII rule 10 (`~/.terrashift/config.toml`
//! is the only sanctioned credential location).
//!
//! ## Stage 1 status
//!
//! - **Working today**: `StubBroker` (in-memory canned), `substitute`
//!   (`{{secret:NAME}}` → value with `SubstitutionMap` for reverse
//!   rebuild), `pre_llm_check` (gitleaks-pattern outbound scrubber via
//!   `terrashift-audit::scrubber`), `Credential` `Zeroizing`-wrapped
//!   with custom `Debug` that never echoes the value, audit emission
//!   of `AuditPayload::CredentialResolution` on every fetch.
//! - **`NotImplementedYet { session: "S5" }`**: `AwsBroker::fetch_aws`
//!   (STS AssumeRole), `GcpBroker::fetch_gcp` (ADC + WIF),
//!   `AzureBroker::fetch_azure` (managed identity / service principal).

pub mod aws;
pub mod azure;
pub mod broker;
pub mod errors;
pub mod federated;
pub mod gcp;
pub mod scrub;
pub mod stub;
pub mod substitution;

pub use aws::AwsBroker;
pub use azure::AzureBroker;
pub use broker::{Credential, CredentialBroker};
pub use errors::CredsError;
pub use federated::{FederatedTokenProvider, OidcIdToken, StubFederatedTokenProvider};
pub use gcp::GcpBroker;
pub use scrub::{pre_llm_check, pre_llm_check_many};
pub use stub::StubBroker;
pub use substitution::{substitute, SubstitutionMap};
