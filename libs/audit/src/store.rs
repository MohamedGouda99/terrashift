// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Append-only audit store — SQLite-backed with hash chain + signing.
//!
//! Pattern: terrashift_plan.md §6.X (signed hash-chained log).
//! Constitution: Article IX (append-only, never auto-deleted), V.

use crate::entry::{AuditEntry, AuditPayload, Outcome};
use crate::errors::AuditError;
use crate::scrubber;
use crate::signer::SessionSigner;
use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::instrument;
use uuid::Uuid;

const SCHEMA_DDL: &str = "
CREATE TABLE IF NOT EXISTS audit_runs (
    run_id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL,
    started_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_entries (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    actor_json TEXT NOT NULL,
    operation TEXT NOT NULL,
    outcome_json TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    prev_hash BLOB NOT NULL,
    content_hash BLOB NOT NULL,
    signature BLOB NOT NULL,
    seq INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_entries_run_seq ON audit_entries(run_id, seq);
";

#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Register a new run with its verify key. Must be called before append().
    async fn register_run(
        &self,
        run_id: Uuid,
        verify_key_bytes: [u8; 32],
    ) -> Result<(), AuditError>;

    /// Append an entry. Computes prev_hash, content_hash, signature
    /// internally; mutates the input. Panics if the scrubber detects a raw
    /// secret in any string field — this is the LAST line of defence
    /// (Article XIII rule 5; terrashift_plan.md §6.X).
    async fn append(
        &self,
        signer: &SessionSigner,
        entry: &mut AuditEntry,
    ) -> Result<(), AuditError>;

    /// Read all entries for a run, oldest first.
    async fn export(&self, run_id: Uuid) -> Result<Vec<AuditEntry>, AuditError>;

    /// Look up the verify key registered with this run.
    async fn get_verify_key(&self, run_id: Uuid) -> Result<[u8; 32], AuditError>;
}

/// Sqlite-backed append-only store.
pub struct LocalAuditStore {
    pool: SqlitePool,
    /// Per-run last entry's content_hash, used as the next prev_hash. Held
    /// in memory because the SQL "find last entry" is per-run lookup; cache
    /// avoids a query on every append.
    chain_heads: Arc<RwLock<std::collections::HashMap<Uuid, [u8; 32]>>>,
}

impl LocalAuditStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, AuditError> {
        let opts = SqliteConnectOptions::new()
            .filename(path.as_ref())
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(opts).await?;
        Self::init(&pool).await?;
        Ok(Self {
            pool,
            chain_heads: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    pub async fn in_memory() -> Result<Self, AuditError> {
        let opts =
            SqliteConnectOptions::from_str("sqlite::memory:").map_err(AuditError::Storage)?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        Self::init(&pool).await?;
        Ok(Self {
            pool,
            chain_heads: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    async fn init(pool: &SqlitePool) -> Result<(), AuditError> {
        sqlx::query(SCHEMA_DDL).execute(pool).await?;
        Ok(())
    }

    async fn next_seq(&self, run_id: Uuid) -> Result<i64, AuditError> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT MAX(seq) FROM audit_entries WHERE run_id = ?1")
                .bind(run_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        let max = row.map(|(x,)| x).unwrap_or(0);
        Ok(max + 1)
    }
}

#[async_trait]
impl AuditStore for LocalAuditStore {
    #[instrument(skip(self), fields(run_id = %run_id))]
    async fn register_run(
        &self,
        run_id: Uuid,
        verify_key_bytes: [u8; 32],
    ) -> Result<(), AuditError> {
        sqlx::query(
            "INSERT OR IGNORE INTO audit_runs (run_id, public_key, started_at) VALUES (?1, ?2, ?3)",
        )
        .bind(run_id.to_string())
        .bind(verify_key_bytes.as_slice())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[instrument(skip(self, signer, entry), fields(run_id = %entry.run_id, op = %entry.operation))]
    async fn append(
        &self,
        signer: &SessionSigner,
        entry: &mut AuditEntry,
    ) -> Result<(), AuditError> {
        // 1. Pre-write redaction scrub — the LAST line of defence
        scrub_or_panic(entry);

        // 2. Compute prev_hash from cached chain head (or zeros for first entry)
        let prev_hash = {
            let heads = self.chain_heads.read().await;
            *heads.get(&entry.run_id).unwrap_or(&[0u8; 32])
        };
        entry.prev_hash = prev_hash;

        // 3. Compute content hash (everything except content_hash + signature)
        entry.content_hash = compute_content_hash(entry)?;

        // 4. Sign the content hash
        entry.signature = signer.sign(&entry.content_hash);

        // 5. Insert
        let seq = self.next_seq(entry.run_id).await?;
        let actor_json = serde_json::to_string(&entry.actor)?;
        let outcome_json = serde_json::to_string(&entry.outcome)?;
        let payload_json = serde_json::to_string(&entry.payload)?;

        sqlx::query(
            "INSERT INTO audit_entries (id, run_id, timestamp, actor_json, operation,
                                        outcome_json, payload_json, prev_hash, content_hash,
                                        signature, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(entry.id.to_string())
        .bind(entry.run_id.to_string())
        .bind(entry.timestamp.to_rfc3339())
        .bind(actor_json)
        .bind(&entry.operation)
        .bind(outcome_json)
        .bind(payload_json)
        .bind(entry.prev_hash.as_slice())
        .bind(entry.content_hash.as_slice())
        .bind(entry.signature.as_slice())
        .bind(seq)
        .execute(&self.pool)
        .await?;

        // 6. Advance chain head
        let mut heads = self.chain_heads.write().await;
        heads.insert(entry.run_id, entry.content_hash);

        Ok(())
    }

    #[instrument(skip(self), fields(run_id = %run_id))]
    async fn export(&self, run_id: Uuid) -> Result<Vec<AuditEntry>, AuditError> {
        let rows: Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Vec<u8>,
            Vec<u8>,
            Vec<u8>,
        )> = sqlx::query_as(
            "SELECT id, run_id, timestamp, actor_json, operation, outcome_json,
                        payload_json, prev_hash, content_hash, signature
                 FROM audit_entries WHERE run_id = ?1 ORDER BY seq ASC",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for (id_s, run_s, ts_s, actor_s, op, outcome_s, payload_s, ph, ch, sig) in rows {
            entries.push(AuditEntry {
                id: Uuid::parse_str(&id_s).map_err(|e| AuditError::Signing(e.to_string()))?,
                run_id: Uuid::parse_str(&run_s).map_err(|e| AuditError::Signing(e.to_string()))?,
                timestamp: ts_s
                    .parse()
                    .map_err(|e: chrono::ParseError| AuditError::Signing(e.to_string()))?,
                actor: serde_json::from_str(&actor_s)?,
                operation: op,
                outcome: serde_json::from_str(&outcome_s)?,
                payload: serde_json::from_str(&payload_s)?,
                prev_hash: ph
                    .as_slice()
                    .try_into()
                    .map_err(|_| AuditError::Signing("prev_hash != 32 bytes".to_string()))?,
                content_hash: ch
                    .as_slice()
                    .try_into()
                    .map_err(|_| AuditError::Signing("content_hash != 32 bytes".to_string()))?,
                signature: sig,
            });
        }
        Ok(entries)
    }

    async fn get_verify_key(&self, run_id: Uuid) -> Result<[u8; 32], AuditError> {
        let row: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT public_key FROM audit_runs WHERE run_id = ?1")
                .bind(run_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        match row {
            Some((bytes,)) => bytes
                .as_slice()
                .try_into()
                .map_err(|_| AuditError::InvalidKey("verify key != 32 bytes".to_string())),
            None => Err(AuditError::RunNotFound {
                run_id: run_id.to_string(),
            }),
        }
    }
}

/// SHA-256 over the canonical serialization of every field EXCEPT
/// content_hash + signature. prev_hash IS included so chain breaks are
/// detectable.
pub fn compute_content_hash(entry: &AuditEntry) -> Result<[u8; 32], AuditError> {
    // Canonical JSON: deterministic key ordering. We use a tuple instead of
    // a struct to guarantee field order.
    let canonical = serde_json::to_vec(&(
        entry.id,
        entry.timestamp.to_rfc3339(),
        entry.run_id,
        &entry.actor,
        &entry.operation,
        &entry.outcome,
        &entry.payload,
        entry.prev_hash.as_slice(),
    ))?;
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    let result: [u8; 32] = hasher
        .finalize()
        .as_slice()
        .try_into()
        .map_err(|_| AuditError::Signing("Sha256 produced wrong length".to_string()))?;
    Ok(result)
}

/// Pre-write scrub — checks every string field of the entry. Panics on
/// detection per terrashift_plan.md §6.X discipline ("the writer is the
/// LAST line of defence; panics loudly").
fn scrub_or_panic(entry: &AuditEntry) {
    let check = |path: &str, value: &str| {
        if let Some(m) = scrubber::scan(path, value) {
            // Loud failure: panic. Per Article XIII rule 5, this is non-bypassable.
            panic!(
                "[AUDIT] secret pattern '{}' detected in field '{}' — REJECTED to prevent leak. Article XIII rule 5.",
                m.pattern_name, m.field_path
            );
        }
    };

    check("entry.operation", &entry.operation);
    if let Outcome::Err { kind, message } = &entry.outcome {
        check("entry.outcome.kind", kind);
        check("entry.outcome.message", message);
    }
    match &entry.payload {
        AuditPayload::LlmCall {
            provider,
            model_id,
            provider_endpoint,
            ..
        } => {
            check("payload.provider", provider);
            check("payload.model_id", model_id);
            check("payload.provider_endpoint", provider_endpoint);
        }
        AuditPayload::ToolExecution {
            tool_name,
            cred_ref,
            target,
            ..
        } => {
            check("payload.tool_name", tool_name);
            if let Some(cr) = cred_ref {
                check("payload.cred_ref", cr);
            }
            check("payload.target", target);
        }
        AuditPayload::PhaseTransition { from, to, .. } => {
            check("payload.from", from);
            check("payload.to", to);
        }
        AuditPayload::CredentialResolution {
            cred_ref,
            resolution_method,
            ..
        } => {
            check("payload.cred_ref", cred_ref);
            check("payload.resolution_method", resolution_method);
        }
        AuditPayload::FileOperation {
            path, backup_path, ..
        } => {
            check("payload.path", &path.display().to_string());
            if let Some(bp) = backup_path {
                check("payload.backup_path", &bp.display().to_string());
            }
        }
    }
}
