// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! `StubBroker` — Stage 1 in-memory canned-credential broker.
//!
//! Pattern: deterministic test fixture; no network, no disk I/O.
//! Constitution: Article V (raw values stored only in `Zeroizing`
//! wrapper; never logged; audit records the reference, not the value),
//! Article XIII rule 7 (in-memory only — no disk-bound secrets in
//! Stage 1 brokers).

use crate::broker::{Credential, CredentialBroker};
use crate::errors::CredsError;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use terrashift_audit::{Actor, AuditEntry, AuditPayload, AuditStore, Outcome, SessionSigner};
use uuid::Uuid;

/// Bundle of audit-emission state. Constructed via `StubBroker::with_audit`.
struct AuditConfig {
    store: Arc<dyn AuditStore + Send + Sync>,
    run_id: Uuid,
    signer: Arc<SessionSigner>,
}

/// `StubBroker` — Stage 1 in-memory broker.
///
/// Builder usage:
///
/// ```ignore
/// let broker = StubBroker::new()
///     .with_secret("aws-prod-deploy", "<fake-aws-token>")
///     .with_secret("gcp-prod", "<fake-gcp-token>")
///     .with_audit(audit_store, run_id, signer);
/// ```
///
/// (Docstring uses placeholder strings rather than realistic-shaped
/// fake credentials — the pre-commit hook's gitleaks regex set is
/// deliberately strict, and even doc examples that match `ya29.*` /
/// `AKIA*` would block the commit. Article V belt-and-braces.)
pub struct StubBroker {
    canned: HashMap<String, String>,
    audit: Option<AuditConfig>,
}

impl StubBroker {
    pub fn new() -> Self {
        Self {
            canned: HashMap::new(),
            audit: None,
        }
    }

    /// Register a canned `(reference name → value)` pair.
    pub fn with_secret(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.canned.insert(name.into(), value.into());
        self
    }

    /// Wire an audit store + run_id + signer; every successful fetch
    /// appends `AuditPayload::CredentialResolution`. The signer is
    /// passed by `Arc` so callers can clone-and-share it across the
    /// broker, the agent loop, etc. The store's `register_run` MUST
    /// have been called with `signer.verify_key_bytes()` before this
    /// builder runs.
    pub fn with_audit(
        mut self,
        store: Arc<dyn AuditStore + Send + Sync>,
        run_id: Uuid,
        signer: Arc<SessionSigner>,
    ) -> Self {
        self.audit = Some(AuditConfig {
            store,
            run_id,
            signer,
        });
        self
    }

    async fn audit_fetch(&self, cred_ref: &str, method: &str) -> Result<(), CredsError> {
        if let Some(cfg) = &self.audit {
            let mut entry = AuditEntry::new(
                cfg.run_id,
                Actor::System,
                "creds.resolve",
                Outcome::Ok,
                AuditPayload::CredentialResolution {
                    cred_ref: cred_ref.to_string(),
                    resolution_method: method.to_string(),
                    expires_at: Utc::now() + Duration::hours(1),
                },
            );
            cfg.store
                .append(&cfg.signer, &mut entry)
                .await
                .map_err(|e| CredsError::Audit(Box::new(e)))?;
        }
        Ok(())
    }
}

impl Default for StubBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialBroker for StubBroker {
    async fn resolve(&self, name: &str) -> Result<Credential, CredsError> {
        let value = self
            .canned
            .get(name)
            .ok_or_else(|| CredsError::UnknownReference(name.to_string()))?
            .clone();
        self.audit_fetch(&format!("{{{{secret:{name}}}}}"), "stub")
            .await?;
        Ok(Credential::new(
            value,
            "stub",
            Utc::now() + Duration::hours(1),
        ))
    }

    async fn fetch_aws(&self, role: &str) -> Result<Credential, CredsError> {
        self.resolve(role).await
    }

    async fn fetch_gcp(&self, account: &str) -> Result<Credential, CredsError> {
        self.resolve(account).await
    }

    async fn fetch_azure(&self, subscription: &str) -> Result<Credential, CredsError> {
        self.resolve(subscription).await
    }
}
