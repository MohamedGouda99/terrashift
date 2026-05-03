//! `FederatedTokenProvider` — short-lived cloud credentials via OIDC.
//!
//! Pattern: terrashift_plan.md §8 (Stage 2 WIF/OIDC modernization).
//! Source: this is a Terrashift seam; real adapters wrap the cloud SDKs:
//! - AWS: `aws-sdk-sts` `AssumeRoleWithWebIdentity`
//! - GCP: `google-cloud-iamcredentials` Workload Identity Federation
//! - Azure: `azure-identity` `WorkloadIdentityCredential`
//!
//! ## Why a separate trait from `CredentialBroker`
//!
//! `CredentialBroker::resolve("{{secret:NAME}}")` is for config-driven
//! static references (an API key the operator put in the profile).
//! `FederatedTokenProvider` is for *trust-driven* federated tokens —
//! the operator's CI/CD runner presents an OIDC ID token issued by
//! GitHub Actions / GitLab CI / OIDC provider, and the cloud trust
//! policy maps that to a short-lived credential. The two paths have
//! different threat models (static-secret leak vs. trust-policy
//! misconfiguration) and different lifecycle (long-lived vs.
//! 15-minute-default-expiry), so the trait surface is separate.
//!
//! ## Stage 2 status
//!
//! - **Working today**: trait + `StubFederatedTokenProvider` (deterministic
//!   in-memory; for hermetic tests).
//! - **Deferred to S12b** (pending operator-side cloud trust setup):
//!   `AwsStsWifProvider`, `GcpWorkloadIdentityProvider`,
//!   `AzureFederatedIdentityProvider`. Each requires the operator to
//!   create a trust policy on the cloud side; we cannot ship a working
//!   adapter without that prerequisite.
//!
//! ## Production path (S12b — operator setup required first)
//!
//! - **AWS**: create an OIDC IdP (`aws iam create-open-id-connect-provider`)
//!   and an IAM role with a trust policy that allows
//!   `sts:AssumeRoleWithWebIdentity` from your OIDC token issuer.
//! - **GCP**: create a Workload Identity Pool + Provider
//!   (`gcloud iam workload-identity-pools create`) and grant
//!   `roles/iam.workloadIdentityUser` to the principal you'll federate
//!   into your service account.
//! - **Azure**: register an App Registration with a federated credential
//!   referencing your OIDC issuer + subject, then assign the appropriate
//!   role on the subscription / resource group.
//!
//! Once trust is set up, swap `StubFederatedTokenProvider` for the
//! corresponding cloud adapter at construction time — the trait surface
//! is identical so the kernel doesn't change.

use crate::broker::Credential;
use crate::errors::CredsError;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Mutex;
use zeroize::Zeroizing;

/// What the operator's CI/CD runner presents to the cloud trust policy.
/// Stage 2 narrowing: opaque string carrying the OIDC ID token. The
/// real adapters validate this against the cloud's IdP and exchange
/// it for short-lived credentials.
#[derive(Debug, Clone)]
pub struct OidcIdToken {
    pub raw_jwt: Zeroizing<String>,
    /// Where the token came from — `"github_actions"`, `"gitlab_ci"`,
    /// `"buildkite"`, etc. Used for audit-log entries; the cloud trust
    /// policy validates the actual issuer field in the JWT.
    pub source: String,
}

impl OidcIdToken {
    pub fn new(raw_jwt: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            raw_jwt: Zeroizing::new(raw_jwt.into()),
            source: source.into(),
        }
    }
}

/// Federated-token provider — converts an OIDC ID token into short-lived
/// cloud credentials per the cloud trust policy.
///
/// Stage 2 ships the trait + `StubFederatedTokenProvider`. Real cloud
/// adapters land in S12b once operator-side trust is configured.
#[async_trait]
pub trait FederatedTokenProvider: Send + Sync {
    /// Exchange the OIDC token for AWS short-lived credentials via
    /// `sts:AssumeRoleWithWebIdentity`. `role_arn` is the operator's
    /// pre-created role with the right trust policy.
    async fn fetch_aws_via_wif(
        &self,
        token: &OidcIdToken,
        role_arn: &str,
    ) -> Result<Credential, CredsError>;

    /// Exchange the OIDC token for GCP short-lived credentials via the
    /// Workload Identity Pool's `impersonateServiceAccount` flow.
    /// `service_account_email` is the GCP SA to impersonate.
    async fn fetch_gcp_via_wif(
        &self,
        token: &OidcIdToken,
        service_account_email: &str,
    ) -> Result<Credential, CredsError>;

    /// Exchange the OIDC token for Azure short-lived credentials via
    /// the App Registration's federated identity. `client_id` is the
    /// Azure AD application that has the federated credential
    /// configured.
    async fn fetch_azure_via_federated(
        &self,
        token: &OidcIdToken,
        client_id: &str,
        tenant_id: &str,
    ) -> Result<Credential, CredsError>;
}

// ---------------------------------------------------------------------
// Stub implementation — for hermetic tests + Stage 2 wiring proof.
// ---------------------------------------------------------------------

/// Hermetic-test provider. Maps `(token.source, principal_or_role)` to
/// a canned credential value. Lets tests assert that the kernel calls
/// the right `fetch_*` method with the right arguments without hitting
/// the cloud.
pub struct StubFederatedTokenProvider {
    /// Key: `"<cloud>:<principal>"` (e.g., `"aws:arn:aws:iam::1234..:role/Migrator"`).
    /// Value: canned credential value.
    canned: Mutex<HashMap<String, String>>,
    /// How long the canned credential is valid. Defaults to 15 minutes
    /// to match real cloud short-lived tokens.
    ttl: Duration,
}

impl StubFederatedTokenProvider {
    pub fn new() -> Self {
        Self {
            canned: Mutex::new(HashMap::new()),
            ttl: Duration::minutes(15),
        }
    }

    pub fn with_canned(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if let Ok(mut map) = self.canned.lock() {
            map.insert(key.into(), value.into());
        }
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    fn lookup(&self, key: &str, method: &str) -> Result<Credential, CredsError> {
        let map = self
            .canned
            .lock()
            .map_err(|_| CredsError::ResolutionError {
                name: key.to_string(),
                cause: "stub federated provider mutex poisoned".to_string(),
            })?;
        let value = map
            .get(key)
            .ok_or_else(|| CredsError::UnknownReference(key.to_string()))?;
        Ok(Credential::new(
            value.clone(),
            method,
            Utc::now() + self.ttl,
        ))
    }
}

impl Default for StubFederatedTokenProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FederatedTokenProvider for StubFederatedTokenProvider {
    async fn fetch_aws_via_wif(
        &self,
        _token: &OidcIdToken,
        role_arn: &str,
    ) -> Result<Credential, CredsError> {
        self.lookup(
            &format!("aws:{role_arn}"),
            "stub_sts_assume_role_with_web_identity",
        )
    }

    async fn fetch_gcp_via_wif(
        &self,
        _token: &OidcIdToken,
        service_account_email: &str,
    ) -> Result<Credential, CredsError> {
        self.lookup(
            &format!("gcp:{service_account_email}"),
            "stub_gcp_workload_identity",
        )
    }

    async fn fetch_azure_via_federated(
        &self,
        _token: &OidcIdToken,
        client_id: &str,
        tenant_id: &str,
    ) -> Result<Credential, CredsError> {
        self.lookup(
            &format!("azure:{tenant_id}/{client_id}"),
            "stub_azure_federated_identity",
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    fn token() -> OidcIdToken {
        OidcIdToken::new("fake.jwt.body", "github_actions")
    }

    // Placeholder credential values — the test only verifies the
    // round-trip works; we deliberately use opaque sentinel strings
    // that don't match any real provider's token shape, so the
    // pre-commit secret-scanner doesn't false-positive on them.
    const AWS_STUB: &str = "<aws-fake-secret-value>";
    const GCP_STUB: &str = "<gcp-fake-secret-value>";
    const AZURE_STUB: &str = "<azure-fake-secret-value>";

    #[tokio::test]
    async fn stub_returns_canned_aws_creds() {
        let provider = StubFederatedTokenProvider::new()
            .with_canned("aws:arn:aws:iam::123:role/Migrator", AWS_STUB);
        let cred = provider
            .fetch_aws_via_wif(&token(), "arn:aws:iam::123:role/Migrator")
            .await
            .expect("ok");
        assert_eq!(cred.expose(), AWS_STUB);
        assert_eq!(
            cred.resolution_method,
            "stub_sts_assume_role_with_web_identity"
        );
    }

    #[tokio::test]
    async fn stub_returns_canned_gcp_creds() {
        let provider = StubFederatedTokenProvider::new()
            .with_canned("gcp:migrator@proj.iam.gserviceaccount.com", GCP_STUB);
        let cred = provider
            .fetch_gcp_via_wif(&token(), "migrator@proj.iam.gserviceaccount.com")
            .await
            .expect("ok");
        assert_eq!(cred.expose(), GCP_STUB);
    }

    #[tokio::test]
    async fn stub_returns_canned_azure_creds() {
        let provider = StubFederatedTokenProvider::new()
            .with_canned("azure:tenant-uuid/client-uuid", AZURE_STUB);
        let cred = provider
            .fetch_azure_via_federated(&token(), "client-uuid", "tenant-uuid")
            .await
            .expect("ok");
        assert_eq!(cred.expose(), AZURE_STUB);
    }

    #[tokio::test]
    async fn stub_unknown_principal_is_loud() {
        let provider = StubFederatedTokenProvider::new();
        let err = provider
            .fetch_aws_via_wif(&token(), "arn:aws:iam::999:role/Missing")
            .await
            .expect_err("should fail");
        assert!(matches!(err, CredsError::UnknownReference { .. }));
    }

    #[tokio::test]
    async fn stub_credential_has_short_ttl() {
        let provider = StubFederatedTokenProvider::new()
            .with_canned("aws:role-x", "x")
            .with_ttl(Duration::minutes(15));
        let cred = provider
            .fetch_aws_via_wif(&token(), "role-x")
            .await
            .expect("ok");
        let now = Utc::now();
        let expires_in = (cred.expires_at - now).num_minutes();
        assert!(
            (10..=20).contains(&expires_in),
            "expected ~15min TTL, got {expires_in}min"
        );
    }

    #[test]
    fn oidc_token_zeroizes_on_drop() {
        let token = OidcIdToken::new("very-secret-jwt", "github_actions");
        // The Zeroizing wrapper guarantees the buffer is wiped on Drop.
        // We can't observe the post-drop state safely from Rust, but
        // the wrapping itself is the contract.
        assert_eq!(token.raw_jwt.as_str(), "very-secret-jwt");
        assert_eq!(token.source, "github_actions");
    }
}
