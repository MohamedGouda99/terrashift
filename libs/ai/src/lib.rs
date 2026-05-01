//! Provider-agnostic LLM client wrapping `stakai`.
//!
//! Pattern: stakpak_arch.md section 10 (stakai SDK — provider trait, model
//! resolution, streaming), section 11 (RunOverrides merge order), section 15
//! (configuration types).
//!
//! Constitution: Article V (LLM provider keys treated as credentials, not
//! config strings), Article X (every LLM call gets a tracing span with
//! provider + model_id attributes), Article XII rule 3 (tier-aware routing),
//! Article XIII rules 5 + 10.
//!
//! Modules to be filled in by P-NN prompts:
//! - `client.rs` — wraps stakai's Inference (P-03)
//! - `resolver.rs` — 5-layer model resolution chain (P-03, per section 11)
//! - `routing.rs` — tier routing: eco / smart (P-03, per Article XII rule 3)
//! - `caching.rs` — multi-tier cache (L1 mem, L2 SQLite, L3 LanceDB)
//! - `streaming.rs` — SSE handling via reqwest-eventsource
//! - `prompt_cache.rs` — Anthropic prompt caching wrapper

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
