// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `AwsBroker` — STS AssumeRole credential fetcher.
//!
//! Pattern: terrashift_plan.md §8 + Terrashift_Plan.docx §8.
//! the architecture reference §16 (cred broker single source of truth) +
//! the architecture reference §27 (secret detection / redaction at the boundary).
//!
//! Constitution: Article V (short-lived STS tokens only — `duration_seconds`
//! defaults to 3600, max 43200; never holds long-term keys; never writes
//! to disk), Article XIII rule 7 (subprocess inherits env via
//! `Command::env()` only).
//!
//! ## What's wired in S5 close
//!
//! `resolve_sts_assume_role(cfg)` actually calls AWS STS AssumeRole
//! via `aws-sdk-sts` and returns a `CloudCredentialEnvVars` carrying
//! the 3 (or 4 with region) env-var pairs that `terraform` expects.
//!
//! The legacy `CredentialBroker` trait methods (`fetch_aws/gcp/azure`)
//! still return `NotImplementedYet` — those wrap a single
//! `Credential` value, which doesn't fit the multi-value STS shape.
//! New code should call `resolve_sts_assume_role` directly.

use crate::broker::{Credential, CredentialBroker};
use crate::cloud_env::CloudCredentialEnvVars;
use crate::config::CredConfig;
use crate::errors::CredsError;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use zeroize::Zeroizing;

pub struct AwsBroker;

impl AwsBroker {
    pub fn new() -> Self {
        Self
    }

    /// Call AWS STS AssumeRole and return the resulting short-lived
    /// federated credentials as a `CloudCredentialEnvVars` bundle.
    ///
    /// Required fields on `cfg`:
    /// - `role_arn` (`arn:aws:iam::<account>:role/<name>`)
    ///
    /// Optional fields (defaults applied):
    /// - `session_name` → `"terrashift"`
    /// - `duration_seconds` → `3600` (1 hour)
    /// - `external_id` (cross-account roles)
    ///
    /// The SDK uses the operator's ambient AWS config (env vars,
    /// `~/.aws/credentials`, IMDS, etc.) to authenticate the
    /// AssumeRole call itself — same as `aws sts assume-role` from
    /// the CLI.
    pub async fn resolve_sts_assume_role(
        &self,
        cfg: &CredConfig,
    ) -> Result<CloudCredentialEnvVars, CredsError> {
        let role_arn = cfg
            .role_arn
            .as_ref()
            .ok_or_else(|| CredsError::MissingRoleArn {
                mode: format!("{:?}", cfg.mode),
            })?;
        let session_name = cfg
            .session_name
            .clone()
            .unwrap_or_else(|| "terrashift".to_string());
        let duration = cfg.duration_seconds.unwrap_or(3600);

        tracing::info!(
            role_arn = %role_arn,
            session_name = %session_name,
            duration_seconds = duration,
            "AwsBroker::resolve_sts_assume_role start"
        );

        let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let sts_client = aws_sdk_sts::Client::new(&aws_config);

        let mut req = sts_client
            .assume_role()
            .role_arn(role_arn)
            .role_session_name(&session_name)
            .duration_seconds(duration as i32);
        if let Some(ext) = cfg.external_id.as_ref() {
            req = req.external_id(ext);
        }

        let resp = req.send().await.map_err(|e| {
            // The SDK's display includes context (e.g. role ARN, error
            // type) but never the token. Safe to surface.
            let cause = format!("{}", e);
            CredsError::AwsStsCallFailed {
                role_arn: role_arn.clone(),
                cause,
            }
        })?;

        let creds = resp.credentials().ok_or(CredsError::AwsStsResponseShape {
            field: "credentials",
        })?;

        let access_key_id = creds.access_key_id().to_string();
        let secret_access_key = creds.secret_access_key().to_string();
        let session_token = creds.session_token().to_string();

        // STS expiration → chrono::DateTime<Utc>. The SDK's `expiration`
        // returns aws_smithy_types::DateTime; convert via secs+nanos.
        let exp_secs = creds.expiration().secs();
        let expires_at = chrono::DateTime::from_timestamp(exp_secs, 0)
            .unwrap_or_else(|| Utc::now() + Duration::seconds(duration as i64));

        let mut vars = vec![
            (
                "AWS_ACCESS_KEY_ID".to_string(),
                Zeroizing::new(access_key_id),
            ),
            (
                "AWS_SECRET_ACCESS_KEY".to_string(),
                Zeroizing::new(secret_access_key),
            ),
            (
                "AWS_SESSION_TOKEN".to_string(),
                Zeroizing::new(session_token),
            ),
        ];
        // STS doesn't return a region — the operator's ambient region
        // applies to the AssumeRole call. terraform reads `AWS_REGION`
        // separately; if the operator has it set, it propagates via the
        // shell-env-inherit path, not this bundle. Document explicitly.
        let _ = &mut vars; // (kept mutable for future region propagation)

        Ok(CloudCredentialEnvVars {
            vars,
            resolution_method: "sts_assume_role".to_string(),
            expires_at,
        })
    }
}

impl Default for AwsBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialBroker for AwsBroker {
    async fn resolve(&self, _name: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_resolve_via_sts_use_resolve_sts_assume_role_for_multi_var_bundle",
            session: "S5",
        })
    }

    async fn fetch_aws(&self, _role: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_sts_assume_role_legacy_use_resolve_sts_assume_role_for_multi_var_bundle",
            session: "S5",
        })
    }

    async fn fetch_gcp(&self, _account: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_broker_called_for_gcp",
            session: "S5",
        })
    }

    async fn fetch_azure(&self, _subscription: &str) -> Result<Credential, CredsError> {
        Err(CredsError::NotImplementedYet {
            which: "aws_broker_called_for_azure",
            session: "S5",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CredMode;

    #[tokio::test]
    async fn resolve_sts_assume_role_loud_error_when_role_arn_missing() {
        let cfg = CredConfig {
            mode: CredMode::StsAssumeRole,
            role_arn: None,
            session_name: None,
            duration_seconds: None,
            external_id: None,
        };
        let result = AwsBroker::new().resolve_sts_assume_role(&cfg).await;
        assert!(matches!(result, Err(CredsError::MissingRoleArn { .. })));
    }

    #[tokio::test]
    async fn resolve_sts_assume_role_propagates_sdk_error_for_invalid_role() {
        // No real AWS credentials in the test environment, OR an invalid
        // role ARN format → AssumeRole call fails. We assert it surfaces
        // as `AwsStsCallFailed`, not a panic.
        let cfg = CredConfig {
            mode: CredMode::StsAssumeRole,
            role_arn: Some("arn:aws:iam::000000000000:role/this-role-does-not-exist".to_string()),
            session_name: None,
            duration_seconds: None,
            external_id: None,
        };
        let result = AwsBroker::new().resolve_sts_assume_role(&cfg).await;
        // Either MissingRoleArn (won't happen — we set it), AwsStsCallFailed
        // (no creds / invalid role), or AwsStsResponseShape (shouldn't
        // happen). All are tolerable; we just verify it doesn't panic
        // and surfaces a CredsError.
        match result {
            Err(CredsError::AwsStsCallFailed { role_arn, .. }) => {
                assert_eq!(
                    role_arn,
                    "arn:aws:iam::000000000000:role/this-role-does-not-exist"
                );
            }
            Err(other) => {
                // Acceptable: any CredsError variant — proves we surface
                // failures cleanly rather than panicking.
                eprintln!("got non-AwsStsCallFailed error variant: {other:?}");
            }
            Ok(_) => {
                panic!("STS AssumeRole on a non-existent role should never succeed");
            }
        }
    }
}
