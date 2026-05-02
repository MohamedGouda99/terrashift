//! Failure modes for the LLM client + resolver.
//!
//! Pattern: matches `libs/engine/src/scanner/errors.rs` shape — every
//! variant named, no anonymous strings.
//! Constitution: Article IV (loud failures), Article V (`MissingApiKey`
//! names the env var that wasn't found, never the resolved value).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    /// Profile references a `provider_key` (e.g., `"groq"`) that has
    /// no matching `[providers.<key>]` block.
    #[error("unknown provider '{0}': no [providers.{0}] block in profile")]
    UnknownProvider(String),

    /// Provider's `type` field isn't supported in Stage 1. Currently
    /// only `"openai-compatible"` is wired (covers Groq); other
    /// types arrive in S2+ when we add per-provider drivers.
    #[error("unsupported provider type '{0}': Stage 1 only supports 'openai-compatible' (S2+ adds anthropic/openai/gemini/bedrock)")]
    UnsupportedProviderType(String),

    /// `RealClient` tried to read the env var named in `api_key_env`
    /// at request time and the var wasn't set. Article V invariant:
    /// we name the variable, never the value.
    #[error(
        "missing api key: env var '{env_var}' is not set (referenced by provider '{provider_key}')"
    )]
    MissingApiKey {
        env_var: String,
        provider_key: String,
    },

    /// Caller passed an empty prompt. Surfacing this loudly per
    /// Article IV — empty prompts are upstream Mapper bugs, not
    /// silent no-ops.
    #[error("empty prompt: cannot send a zero-length completion request")]
    EmptyPrompt,

    /// Profile model string doesn't match `<provider_key>/<model_id>`
    /// shape (e.g., missing the `/` separator).
    #[error("malformed model identifier '{0}': expected '<provider_key>/<model_id>' (e.g., 'groq/llama-3.3-70b-versatile')")]
    MalformedModelId(String),

    /// TOML parse error reading the profile file.
    #[error("profile TOML parse: {0}")]
    TomlParse(Box<toml::de::Error>),

    /// Filesystem I/O reading `~/.terrashift/config.toml`.
    #[error("profile I/O: {0}")]
    Io(#[from] std::io::Error),

    /// Wrapped error from the underlying stakai SDK. Boxed because
    /// stakai::Error is large (clippy `result_large_err`).
    #[error("stakai SDK: {0}")]
    Stakai(Box<stakai::Error>),
}
