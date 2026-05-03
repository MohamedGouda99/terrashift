//! `CredentialBroker` trait + `Credential` (Zeroizing-wrapped value).
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! (zeroize discipline) + the architecture reference §27 (broker as the single
//! source of truth at the tool-execution boundary).
//!
//! Constitution: Article V (heart — the broker is the ONLY layer that
//! holds a raw value, and only as `Zeroizing<String>`; LLM never sees
//! it; audit log records the *reference*, not the value),
//! Article XIII rule 7 (no disk-bound secret writes — broker is
//! in-memory; S5 wires real STS / ADC / managed-identity which fetch
//! short-lived tokens, never persist them).

use crate::errors::CredsError;
use async_trait::async_trait;
use zeroize::Zeroizing;

/// A credential value. The inner `String` is wrapped in `Zeroizing` so
/// the underlying byte buffer is zeroed on drop. Construct via
/// `Credential::new(value)`; never log or `Debug`-print the inner
/// value.
pub struct Credential {
    inner: Zeroizing<String>,
    /// Resolution method (`"sts_assume_role"`, `"gcp_adc"`,
    /// `"azure_managed_identity"`, `"stub"`, etc.) — used by the audit
    /// hook to populate `AuditPayload::CredentialResolution`.
    pub resolution_method: String,
    /// When the credential expires (best-effort; STA tokens carry
    /// real expiries, stub uses far-future).
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl Credential {
    pub fn new(
        value: impl Into<String>,
        resolution_method: impl Into<String>,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            inner: Zeroizing::new(value.into()),
            resolution_method: resolution_method.into(),
            expires_at,
        }
    }

    /// Borrow the inner value for a single use. Callers must NEVER
    /// clone or store this `&str` beyond the immediate substitution /
    /// HTTP-header insertion site. Article V.
    pub fn expose(&self) -> &str {
        self.inner.as_str()
    }

    /// Expose the byte length only. For tests verifying zeroization.
    pub fn byte_len(&self) -> usize {
        self.inner.len()
    }
}

// Custom Debug — never echo the inner value.
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("inner", &"<redacted>")
            .field("resolution_method", &self.resolution_method)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// The single trait every cloud-credential implementation honors.
/// `StubBroker` (in-memory canned), `AwsBroker` (STS AssumeRole; S5),
/// `GcpBroker` (ADC + workload identity federation; S5), `AzureBroker`
/// (managed identity / service principal; S5).
///
/// Stage 1: only `StubBroker` returns `Ok`; cloud brokers return
/// `NotImplementedYet`. Stage 5+ wires the real SDK calls.
#[async_trait]
pub trait CredentialBroker: Send + Sync {
    /// Resolve a `{{secret:NAME}}` reference to a live credential.
    /// `name` is the bare reference body (no `{{secret:...}}` wrapper).
    /// Returns `Err(UnknownReference)` if the broker has no entry.
    async fn resolve(&self, name: &str) -> Result<Credential, CredsError>;

    /// Fetch AWS short-lived credentials via STS AssumeRole. Stage 1:
    /// `Err(NotImplementedYet { which: "aws_sts_assume_role", session: "S5" })`.
    async fn fetch_aws(&self, role: &str) -> Result<Credential, CredsError>;

    /// Fetch GCP short-lived credentials via ADC + workload identity
    /// federation. Stage 1: `Err(NotImplementedYet)`.
    async fn fetch_gcp(&self, account: &str) -> Result<Credential, CredsError>;

    /// Fetch Azure short-lived credentials via managed identity or
    /// service principal. Stage 1: `Err(NotImplementedYet)`.
    async fn fetch_azure(&self, subscription: &str) -> Result<Credential, CredsError>;
}
