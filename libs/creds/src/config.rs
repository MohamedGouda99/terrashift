// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `CredConfig` + `CredMode` — operator-supplied cloud credential resolution
//! configuration, deserialized from `~/.terrashift/profile.toml`.
//!
//! Pattern: spec `specs/s5-close-executor/spec.md` US2.
//! Constitution: Article V (config carries references — `mode` selector +
//! ARN/env-var name; never the resolved value), Article XIII rule 7
//! (in-memory only after resolution).
//!
//! ## TOML shape
//!
//! ```toml
//! [profiles.default.creds.aws]
//! mode = "sts_assume_role"
//! role_arn = "arn:aws:iam::123:role/terrashift-deploy"
//! session_name = "terrashift"
//! duration_seconds = 3600
//! external_id = "optional-value"
//!
//! [profiles.default.creds.azurerm]
//! mode = "service_principal_env"
//!
//! [profiles.default.creds.google]
//! mode = "adc"
//! ```

use serde::Deserialize;

/// Per-cloud credential configuration. Populated from `[profiles.X.creds.<cloud>]`.
///
/// Optional fields are mode-specific; the broker dispatcher validates
/// that the right ones are set for the chosen `mode`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CredConfig {
    pub mode: CredMode,

    /// AWS only — required when `mode = "sts_assume_role"`.
    /// Format: `arn:aws:iam::<account>:role/<name>`.
    pub role_arn: Option<String>,

    /// AWS only — STS session name. Defaults to `"terrashift"` when omitted.
    pub session_name: Option<String>,

    /// AWS only — STS session duration. 900..43200 seconds (15min..12h).
    /// Defaults to 3600 (1h).
    pub duration_seconds: Option<u32>,

    /// AWS only — optional external ID for cross-account roles.
    pub external_id: Option<String>,
}

/// How the broker should resolve credentials for a given cloud.
///
/// Stage 1 close ships the modes listed below; each variant maps to one
/// concrete resolution path in `aws::Broker` / `gcp::Broker` /
/// `azure::Broker`. New modes need:
///   1. A new variant here.
///   2. A new branch in the broker's `resolve_for_cloud` dispatcher.
///   3. Backend implementation + tests.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredMode {
    /// AWS STS AssumeRole. Requires `role_arn`. Recommended posture
    /// per Article V (short-lived federated tokens).
    StsAssumeRole,

    /// AWS — read `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` /
    /// optional `AWS_SESSION_TOKEN` from the parent process env.
    /// Less recommended (long-lived keys) but supported for legacy
    /// CI runners.
    StaticEnv,

    /// GCP — Application Default Credentials. Walks the SDK's
    /// own fallback chain (env var → metadata server → gcloud config).
    Adc,

    /// Azure — service principal env vars
    /// (`ARM_CLIENT_ID`, `ARM_CLIENT_SECRET`, `ARM_TENANT_ID`,
    /// `ARM_SUBSCRIPTION_ID`).
    ServicePrincipalEnv,

    /// Azure — managed identity via IMDS endpoint. Useful when
    /// terrashift runs inside an Azure VM / Container Apps / etc.
    ManagedIdentity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aws_sts_assume_role() {
        let toml = r#"
            mode = "sts_assume_role"
            role_arn = "arn:aws:iam::123:role/terrashift-deploy"
            session_name = "terrashift"
            duration_seconds = 3600
        "#;
        let cfg: CredConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.mode, CredMode::StsAssumeRole);
        assert_eq!(
            cfg.role_arn.as_deref(),
            Some("arn:aws:iam::123:role/terrashift-deploy")
        );
        assert_eq!(cfg.duration_seconds, Some(3600));
    }

    #[test]
    fn parse_gcp_adc_minimal() {
        let toml = r#"mode = "adc""#;
        let cfg: CredConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.mode, CredMode::Adc);
        assert!(cfg.role_arn.is_none());
        assert!(cfg.session_name.is_none());
    }

    #[test]
    fn parse_azure_service_principal_env() {
        let toml = r#"mode = "service_principal_env""#;
        let cfg: CredConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.mode, CredMode::ServicePrincipalEnv);
    }

    #[test]
    fn parse_azure_managed_identity() {
        let toml = r#"mode = "managed_identity""#;
        let cfg: CredConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.mode, CredMode::ManagedIdentity);
    }

    #[test]
    fn parse_static_env_legacy_aws() {
        let toml = r#"mode = "static_env""#;
        let cfg: CredConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.mode, CredMode::StaticEnv);
    }

    #[test]
    fn unknown_mode_is_loud_error() {
        let toml = r#"mode = "telepathy""#;
        let result: Result<CredConfig, _> = toml::from_str(toml);
        assert!(result.is_err(), "unknown mode must error");
    }
}
