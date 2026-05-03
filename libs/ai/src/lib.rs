//! Terrashift AI client — wraps `stakai` with BYOK + tier routing.
//!
//! Pattern: terrashift_plan.md §6.3 (LLM Router design) + the architecture reference §10.
//! Source: the reference codebase (see ATTRIBUTIONS.md) — stakai is the an upstream-published
//! SDK we depend on (workspace dep `stakai = "0.3"`); we wrap it in a
//! thin facade.
//!
//! Constitution: Article V (api_key_env references; env-var read at
//! request time, never instance state), Article XII rule 3 (tier-bound
//! API surface), Article XIII rule 3 (no unwrap/expect/string-slice),
//! Article XIII rule 4 (stakai owns Message/Role; we don't redefine).
//!
//! ## Public API
//!
//! ```no_run
//! use terrashift_ai::{Profile, Resolver, RealClient, LlmClient, Tier};
//!
//! # async fn doctest(toml_str: &str) -> Result<(), terrashift_ai::AiError> {
//! let profile = Profile::from_toml(toml_str)?;
//! let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco)?;
//! let client = RealClient::new(resolved)?;
//! let (text, _meta) = client.complete(Tier::Eco, "Hello, world").await?;
//! println!("{}", text);
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//! - `tier`         — `Tier` enum (`Eco | Smart`)
//! - `errors`       — `AiError` enum
//! - `metadata`     — `CompletionMetadata` (audit-friendly fields)
//! - `profile`      — `Profile`, `ProviderConfig`, `Tiers` TOML schema
//! - `resolver`     — 5-layer `Resolver` per terrashift_plan.md §6.X
//! - `client`       — `LlmClient` trait + `StubClient`
//! - `provider`     — `Provider` trait + `build_provider()` registry (§39 row 1)
//! - `providers`    — concrete provider impls: openai_compat (S1), anthropic/gemini/bedrock (S2+ stubs)
//! - `real`         — `RealClient` facade dispatching via the registry
//! - `agent_client` — `JsonAgentLlmClient` adapts `LlmClient` to the S9 agent-kernel `AgentLlmClient` trait

pub mod agent_client;
pub mod client;
pub mod errors;
pub mod metadata;
pub mod profile;
pub mod provider;
pub mod providers;
pub mod real;
pub mod resolver;
pub mod tier;

pub use agent_client::JsonAgentLlmClient;
pub use client::{LlmClient, StubClient};
pub use errors::AiError;
pub use metadata::CompletionMetadata;
pub use profile::{Profile, ProviderConfig, Tiers};
pub use provider::{build_provider, Provider};
pub use real::RealClient;
pub use resolver::{ResolvedModel, Resolver};
pub use tier::Tier;
