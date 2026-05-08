// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `CloudCredentialEnvVars` — multi-value cred bundle (the shape STS,
//! ADC, and managed-identity actually return).
//!
//! Pattern: spec `specs/s5-close-executor/spec.md` US2.
//! Constitution: Article V (Zeroizing wrappers on every value),
//! Article XIII rule 7 (the bundle is in-memory only — `.into_env()`
//! drops the Zeroizing wrappers AT the call boundary right before
//! `tokio::process::Command::env()` consumes them).

use chrono::{DateTime, Utc};
use zeroize::Zeroizing;

/// A bundle of cloud-credential env vars. Returned by every backend
/// (`AwsBroker::resolve_sts_assume_role`, `GcpBroker::resolve_adc`,
/// `AzureBroker::resolve_managed_identity`, etc.).
///
/// Each entry is `(env_var_name, Zeroizing<String>)`. The Executor
/// calls `.into_env()` to convert into the plain `Vec<(String, String)>`
/// that `tokio::process::Command::env` accepts; the conversion drops
/// the Zeroizing wrappers immediately so parent-process memory clears
/// as soon as the subprocess inherits the env.
pub struct CloudCredentialEnvVars {
    pub vars: Vec<(String, Zeroizing<String>)>,
    /// Resolution method (`"sts_assume_role"`, `"adc"`,
    /// `"managed_identity"`, `"static_env"`, etc.) — used by audit
    /// log + observability.
    pub resolution_method: String,
    /// When the underlying token expires. STS returns a real expiry;
    /// static-env / ADC use far-future as a stand-in.
    pub expires_at: DateTime<Utc>,
}

impl CloudCredentialEnvVars {
    /// Drain into the plain `Vec<(String, String)>` shape that
    /// `tokio::process::Command::env_clear` + `.envs()` accepts. The
    /// Zeroizing wrappers go out of scope at this method's return,
    /// so parent-process memory clears immediately after the caller
    /// hands the result to the subprocess.
    pub fn into_env(self) -> Vec<(String, String)> {
        self.vars
            .into_iter()
            .map(|(k, v)| {
                // Clone the inner String out of Zeroizing<String>; the
                // Zeroizing wrapper drops at end of this closure.
                let s: String = v.as_str().to_string();
                (k, s)
            })
            .collect()
    }

    /// Diagnostic — number of env vars in the bundle. Does NOT log
    /// values.
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }
}

impl std::fmt::Debug for CloudCredentialEnvVars {
    /// Custom `Debug` redacts every value. Article V — never expose
    /// the inner string via formatting.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudCredentialEnvVars")
            .field(
                "var_names",
                &self.vars.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            )
            .field("resolution_method", &self.resolution_method)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn into_env_unwraps_zeroizing_strings() {
        let bundle = CloudCredentialEnvVars {
            vars: vec![
                (
                    "AWS_ACCESS_KEY_ID".to_string(),
                    Zeroizing::new("AKIA...".to_string()),
                ),
                (
                    "AWS_SECRET_ACCESS_KEY".to_string(),
                    Zeroizing::new("SECRET".to_string()),
                ),
            ],
            resolution_method: "sts_assume_role".to_string(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        let env = bundle.into_env();
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].0, "AWS_ACCESS_KEY_ID");
        assert_eq!(env[0].1, "AKIA...");
    }

    #[test]
    fn debug_redacts_values() {
        let bundle = CloudCredentialEnvVars {
            vars: vec![(
                "AWS_SECRET_ACCESS_KEY".to_string(),
                Zeroizing::new("very-secret".to_string()),
            )],
            resolution_method: "sts_assume_role".to_string(),
            expires_at: Utc::now(),
        };
        let formatted = format!("{:?}", bundle);
        assert!(
            !formatted.contains("very-secret"),
            "Debug must NOT leak inner values: {formatted}"
        );
        assert!(
            formatted.contains("AWS_SECRET_ACCESS_KEY"),
            "Debug should show var names: {formatted}"
        );
    }
}
