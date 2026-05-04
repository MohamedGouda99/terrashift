// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `Profile` — operator config from `~/.terrashift/config.toml`.
//!
//! Pattern: terrashift_plan.md §6.X (config schema sample) + pre-flight
//! Decision 9 (Groq sample TOML).
//! Constitution: Article V (api_key_env references only — never the
//! resolved value), Article XIII rule 10 (config lives under
//! `~/.terrashift/`; resolver validates at load time).

use crate::errors::AiError;
use crate::tier::Tier;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Top-level: a profile bundle. The active profile is selected via
/// `terrashift --profile <name>`; default is `"default"`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileBundle {
    pub profiles: BTreeMap<String, Profile>,
}

/// One profile — operator-defined. Carries the default model, per-tier
/// model assignments, and the providers each model can be served by.
///
/// Sample TOML (per pre-flight Decision 9):
///
/// ```toml
/// [profiles.default]
/// model = "groq/llama-3.3-70b-versatile"
///
///   [profiles.default.tiers]
///   eco   = "groq/llama-3.3-70b-versatile"
///   smart = "groq/llama-3.3-70b-versatile"
///
///   [profiles.default.providers.groq]
///   type = "openai-compatible"
///   api_endpoint = "https://api.groq.com/openai/v1"
///   api_key_env = "GROQ_API_KEY"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    /// Default model when a tier-specific override isn't set.
    /// Format: `<provider_key>/<model_id>`.
    pub model: Option<String>,
    pub tiers: Tiers,
    pub providers: BTreeMap<String, ProviderConfig>,
}

/// Per-tier model assignments. Stage 1 supports `eco` (required) and
/// `smart` (optional — falls back to `eco` if absent, with a warn).
#[derive(Debug, Clone, Deserialize)]
pub struct Tiers {
    pub eco: String,
    pub smart: Option<String>,
}

/// Per-provider block. Stage 1: only `type = "openai-compatible"` is
/// wired (covers Groq, custom Vodafone gateway, etc.). Other types
/// (`anthropic`, `openai`, `gemini`, `bedrock`) arrive in S2+ when
/// per-provider drivers ship.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub api_endpoint: String,
    /// Environment variable name holding the API key. Resolver validates
    /// the variable exists at request time; `Profile` carries only the
    /// reference. Article V invariant.
    pub api_key_env: String,
}

impl Profile {
    /// Parse a profile from a TOML string. Returns `AiError::TomlParse`
    /// on malformed input.
    pub fn from_toml(s: &str) -> Result<Self, AiError> {
        // The TOML may be either a bare `[profiles.default]` table or
        // a single `Profile` (test fixtures). Try both shapes.
        if let Ok(bundle) = toml::from_str::<ProfileBundle>(s) {
            return bundle
                .profiles
                .get("default")
                .cloned()
                .ok_or_else(|| AiError::UnknownProvider("default".to_string()));
        }
        toml::from_str::<Profile>(s).map_err(|e| AiError::TomlParse(Box::new(e)))
    }

    /// Look up the model identifier for a tier, falling back to `model`
    /// if the tier isn't explicitly set. `Tier::Smart` falls back to
    /// `Tier::Eco` per clarify Q (Stage 1 simplification).
    pub fn model_for_tier(&self, tier: Tier) -> String {
        match tier {
            Tier::Eco => self.tiers.eco.clone(),
            Tier::Smart => self
                .tiers
                .smart
                .clone()
                .unwrap_or_else(|| self.tiers.eco.clone()),
        }
    }
}

/// Split a `<provider_key>/<model_id>` string into its parts.
/// Article IV: malformed input is a loud Err, not a silent split.
pub fn split_model_id(combined: &str) -> Result<(&str, &str), AiError> {
    combined
        .split_once('/')
        .filter(|(p, m)| !p.is_empty() && !m.is_empty())
        .ok_or_else(|| AiError::MalformedModelId(combined.to_string()))
}
