// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Ed25519 signing wrappers.
//!
//! Pattern: terrashift_plan.md §6.X (per-migration key, public key persisted
//! in run metadata so verification works after the fact).
//! Constitution: Article V (audit signing keys are LOCKED — no override anywhere).

use crate::errors::AuditError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

/// A per-migration keypair. Generated at run start; the verify key is
/// persisted in the `audit_runs` table so old entries can be verified later.
pub struct SessionSigner {
    signing_key: SigningKey,
}

impl SessionSigner {
    /// Generate a fresh keypair for a new migration run.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Restore from a stored seed (e.g., when continuing a run after restart).
    /// 32-byte seed required.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(seed),
        }
    }

    /// The 32-byte verify key — store this in audit_runs for later verification.
    pub fn verify_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Sign a content hash, returning the 64-byte signature.
    pub fn sign(&self, content_hash: &[u8; 32]) -> Vec<u8> {
        self.signing_key.sign(content_hash).to_bytes().to_vec()
    }
}

/// Verify a signature against a content hash given the public verify key.
pub fn verify_signature(
    verify_key_bytes: &[u8; 32],
    content_hash: &[u8; 32],
    signature: &[u8],
) -> Result<(), AuditError> {
    let verify_key = VerifyingKey::from_bytes(verify_key_bytes)
        .map_err(|e| AuditError::InvalidKey(e.to_string()))?;
    let sig_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| AuditError::SignatureInvalid {
            entry_id: "(unknown)".to_string(),
            reason: format!("signature length {} != 64", signature.len()),
        })?;
    let sig = Signature::from_bytes(&sig_bytes);
    verify_key
        .verify(content_hash, &sig)
        .map_err(|e| AuditError::SignatureInvalid {
            entry_id: "(unknown)".to_string(),
            reason: e.to_string(),
        })
}
