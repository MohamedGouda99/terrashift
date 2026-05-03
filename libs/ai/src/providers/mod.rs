//! Concrete provider implementations.
//!
//! Pattern: stakpak_arch.md §39 row 1 / refs/stakpak/libs/ai/src/providers/.
//! Each provider lives in its own subdirectory with the canonical
//! `{convert, mod, provider, stream, types}.rs` shape (Stakpak parity).
//!
//! Stage 1 active:
//! - `openai_compat` — Groq, OpenAI, custom OpenAI-shape gateways
//!   (Vodafone-internal, Together, Anyscale, etc.)
//!
//! Stage 2+ stubs (each returns `AiError::UnsupportedProviderType`
//! until its session lands; same pattern as libs/creds/{aws,gcp,azure}):
//! - `anthropic` — S2+
//! - `gemini`    — S2+
//! - `bedrock`   — S3+

pub mod anthropic;
pub mod bedrock;
pub mod gemini;
pub mod openai_compat;
