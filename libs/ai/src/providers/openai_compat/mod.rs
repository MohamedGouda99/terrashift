//! `openai_compat` provider — covers Groq, OpenAI, and any
//! OpenAI-shape gateway (Vodafone-internal, Together, Anyscale,
//! Fireworks, DeepInfra, etc.).
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md) shape, with
//! `provider.rs` carrying the `Provider` trait impl, `convert.rs`
//! the request-shape conversion, `stream.rs` the streaming layer
//! (S5+ stub for Stage 1), and `types.rs` provider-specific
//! request/response types.
//!
//! Constitution: Article II (canonical mapping §39 row 1),
//! Article V (api_key_env read at request time, never instance
//! state — see provider.rs:read_api_key).

pub mod convert;
pub mod provider;
pub mod stream;
pub mod types;

pub use provider::OpenAiCompat;
