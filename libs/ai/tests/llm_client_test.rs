// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Integration tests for P-03 LLM client + BYOK resolver.
//!
//! All tests offline by default. The `real_groq_completes` test is
//! `#[ignore]`'d and ALSO env-var-gated (`GROQ_API_KEY`) so it
//! doesn't run on someone's `cargo test -- --ignored` against an
//! unrelated crate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use terrashift_ai::{AiError, LlmClient, Profile, RealClient, Resolver, StubClient, Tier};

const SAMPLE_PROFILE_TOML: &str = r#"
[profiles.default]
model = "groq/llama-3.3-70b-versatile"

  [profiles.default.tiers]
  eco   = "groq/llama-3.3-70b-versatile"
  smart = "groq/llama-3.3-70b-versatile"

  [profiles.default.providers.groq]
  type = "openai-compatible"
  api_endpoint = "https://api.groq.com/openai/v1"
  api_key_env = "GROQ_API_KEY"
"#;

const SAMPLE_PROFILE_WITH_TWO_PROVIDERS: &str = r#"
[profiles.default]

  [profiles.default.tiers]
  eco   = "groq/llama-3.3-70b-versatile"
  smart = "anthropic/claude-opus-4-7"

  [profiles.default.providers.groq]
  type = "openai-compatible"
  api_endpoint = "https://api.groq.com/openai/v1"
  api_key_env = "GROQ_API_KEY"

  [profiles.default.providers.anthropic]
  type = "anthropic"
  api_endpoint = "https://api.anthropic.com"
  api_key_env = "ANTHROPIC_API_KEY"
"#;

// ─────────────────────────────────────────────────────────────────────
// US1 — Profile resolution
// ─────────────────────────────────────────────────────────────────────

#[test]
fn profile_round_trips_toml() {
    let profile = Profile::from_toml(SAMPLE_PROFILE_TOML).unwrap();
    assert_eq!(profile.tiers.eco, "groq/llama-3.3-70b-versatile");
    assert_eq!(
        profile.tiers.smart.as_deref(),
        Some("groq/llama-3.3-70b-versatile")
    );
    assert!(profile.providers.contains_key("groq"));
    let groq = &profile.providers["groq"];
    assert_eq!(groq.provider_type, "openai-compatible");
    assert_eq!(groq.api_endpoint, "https://api.groq.com/openai/v1");
    assert_eq!(groq.api_key_env, "GROQ_API_KEY");
}

#[test]
fn resolver_picks_tier_from_profile() {
    let profile = Profile::from_toml(SAMPLE_PROFILE_TOML).unwrap();
    let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).unwrap();
    assert_eq!(resolved.provider_key, "groq");
    assert_eq!(resolved.model_id, "llama-3.3-70b-versatile");
    assert_eq!(resolved.provider_type, "openai-compatible");
    assert_eq!(resolved.api_endpoint, "https://api.groq.com/openai/v1");
    assert_eq!(resolved.api_key_env, "GROQ_API_KEY");
    assert_eq!(resolved.tier, Tier::Eco);
}

#[test]
fn cli_override_wins_over_profile() {
    let profile = Profile::from_toml(SAMPLE_PROFILE_WITH_TWO_PROVIDERS).unwrap();
    // Profile says Eco = groq; CLI override forces anthropic.
    let resolved =
        Resolver::resolve_for_tier(&profile, Some("anthropic/claude-opus-4-7"), None, Tier::Eco)
            .unwrap();
    assert_eq!(resolved.provider_key, "anthropic");
    assert_eq!(resolved.model_id, "claude-opus-4-7");
}

#[test]
fn per_call_override_wins_over_cli() {
    let profile = Profile::from_toml(SAMPLE_PROFILE_WITH_TWO_PROVIDERS).unwrap();
    let resolved = Resolver::resolve_for_tier(
        &profile,
        Some("anthropic/claude-opus-4-7"),    // CLI says anthropic
        Some("groq/llama-3.3-70b-versatile"), // per-call says groq
        Tier::Eco,
    )
    .unwrap();
    assert_eq!(resolved.provider_key, "groq");
    assert_eq!(resolved.model_id, "llama-3.3-70b-versatile");
}

#[test]
fn smart_tier_falls_back_to_eco_when_unset() {
    // Profile WITHOUT smart set
    let toml = r#"
[profiles.default]
  [profiles.default.tiers]
  eco = "groq/llama-3.3-70b-versatile"

  [profiles.default.providers.groq]
  type = "openai-compatible"
  api_endpoint = "https://api.groq.com/openai/v1"
  api_key_env = "GROQ_API_KEY"
"#;
    let profile = Profile::from_toml(toml).unwrap();
    let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Smart).unwrap();
    assert_eq!(resolved.model_id, "llama-3.3-70b-versatile");
}

#[test]
fn unknown_provider_is_loud_error() {
    let toml = r#"
[profiles.default]
  [profiles.default.tiers]
  eco = "nonexistent/some-model"

  [profiles.default.providers.groq]
  type = "openai-compatible"
  api_endpoint = "https://api.groq.com/openai/v1"
  api_key_env = "GROQ_API_KEY"
"#;
    let profile = Profile::from_toml(toml).unwrap();
    let err = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("nonexistent"),
        "names the missing provider: {msg}"
    );
    assert!(matches!(err, AiError::UnknownProvider(_)));
}

#[test]
fn malformed_model_id_is_loud_error() {
    let toml = r#"
[profiles.default]
  [profiles.default.tiers]
  eco = "no_slash_in_this_id"

  [profiles.default.providers.groq]
  type = "openai-compatible"
  api_endpoint = "https://api.groq.com/openai/v1"
  api_key_env = "GROQ_API_KEY"
"#;
    let profile = Profile::from_toml(toml).unwrap();
    let err = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).unwrap_err();
    assert!(matches!(err, AiError::MalformedModelId(_)));
}

// ─────────────────────────────────────────────────────────────────────
// US2 — StubClient
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stub_returns_canned_response() {
    let stub = StubClient::new().with_response("aws_vpc", "azurerm_virtual_network");
    let (text, meta) = stub
        .complete(Tier::Eco, "map aws_vpc to azure")
        .await
        .unwrap();
    assert_eq!(text, "azurerm_virtual_network");
    assert_eq!(meta.provider, "stub");
    assert_eq!(meta.tier, "eco");
}

#[tokio::test]
async fn stub_default_fallback() {
    let stub = StubClient::new().with_default("fallback-text");
    let (text, _) = stub.complete(Tier::Smart, "anything").await.unwrap();
    assert_eq!(text, "fallback-text");
}

#[tokio::test]
async fn empty_prompt_is_loud_error() {
    let stub = StubClient::new();
    let err = stub.complete(Tier::Eco, "").await.unwrap_err();
    assert!(matches!(err, AiError::EmptyPrompt));
}

// ─────────────────────────────────────────────────────────────────────
// US3 — RealClient construction (offline construction; real network call
// is the #[ignore]'d test below)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn real_client_rejects_unsupported_provider_type() {
    let profile = Profile::from_toml(SAMPLE_PROFILE_WITH_TWO_PROVIDERS).unwrap();
    let resolved = Resolver::resolve_for_tier(
        &profile,
        Some("anthropic/claude-opus-4-7"),
        None,
        Tier::Smart,
    )
    .unwrap();
    // RealClient doesn't impl Debug (stakai::Inference doesn't either),
    // so we can't `.unwrap_err()`. Pattern-match instead.
    match RealClient::new(resolved) {
        Err(AiError::UnsupportedProviderType(t)) => assert_eq!(t, "anthropic"),
        Err(other) => panic!("expected UnsupportedProviderType, got {:?}", other),
        Ok(_) => panic!("expected Err, got Ok(RealClient)"),
    }
}

#[test]
fn real_client_constructs_for_openai_compatible() {
    let profile = Profile::from_toml(SAMPLE_PROFILE_TOML).unwrap();
    let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).unwrap();
    let _client = RealClient::new(resolved).expect("openai-compatible should construct");
}

// ─────────────────────────────────────────────────────────────────────
// Real Groq integration — #[ignore]'d so default `cargo test` doesn't
// fire. Activate with `cargo test -- --ignored` AND `GROQ_API_KEY` set.
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "real Groq round-trip; requires GROQ_API_KEY env var. Activate with: cargo test -p terrashift-ai real_groq_completes -- --ignored"]
async fn real_groq_completes() {
    // Defensive: avoid surprising someone who runs `cargo test -- --ignored`
    // against an unrelated crate.
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("[skip] GROQ_API_KEY not set; the real Groq test cannot run.");
        return;
    }

    let profile = Profile::from_toml(SAMPLE_PROFILE_TOML).unwrap();
    let resolved = Resolver::resolve_for_tier(&profile, None, None, Tier::Eco).unwrap();
    let client = RealClient::new(resolved).unwrap();
    let (text, meta) = client
        .complete(
            Tier::Eco,
            "Reply with the single word: pong. No other text.",
        )
        .await
        .expect("Groq round-trip should succeed when GROQ_API_KEY is set");
    assert!(!text.is_empty(), "non-empty response");
    assert!(meta.latency_ms > 0, "latency should be measured");
    assert_eq!(meta.provider, "groq");
    assert_eq!(meta.model_id, "llama-3.3-70b-versatile");
}
