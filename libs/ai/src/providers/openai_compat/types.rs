//! Provider-specific request/response types — S5+ stub.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! Stage 1 doesn't need provider-specific shapes: stakai's
//! `GenerateRequest`/`GenerateResponse` cover everything the Mapper
//! does today, and `convert.rs` translates `(Tier, prompt)` directly
//! into stakai types.
//!
//! S5+ this module will hold openai-shape-specific request fields
//! (e.g., `tool_choice`, `response_format` for structured output,
//! `seed` for cache-stable replay).
