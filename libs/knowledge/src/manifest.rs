// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Runtime manifest for the on-disk schema cache.
//!
//! The manifest is the index file at `~/.terrashift/schemas/manifest.json`
//! that records every captured `(provider, version)` pair plus the metadata
//! reviewers and `terrashift schema verify` need to detect drift.
//!
//! Pattern: terrashift_plan.md §7 (knowledge layer); RFC schema-source-migration §4.4.
//! Constitution: Article VI (version pinning), Article IX (append-only writes —
//! `gc` removes whole `(provider, version)` directories but never mutates entries
//! in place).
//!
//! # File layout
//!
//! ```text
//! ~/.terrashift/schemas/
//! ├── manifest.json              ← this file
//! ├── aws/
//! │   └── 5.30.0/
//! │       ├── schema.json
//! │       └── capture.log
//! └── azurerm/
//!     └── 3.110.0/
//!         ├── schema.json
//!         └── capture.log
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::errors::SchemaError;

/// Current manifest format version. Bump on incompatible schema changes;
/// pair with a one-shot migration in `manifest/migrations.rs` (see RFC §3).
pub const SCHEMA_VERSION: u32 = 1;

/// On-disk manifest. Loaded once at process start; updated atomically when
/// `terrashift schema update` adds or refreshes a (provider, version) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeManifest {
    pub schema_version: u32,
    #[serde(default)]
    pub schemas: Vec<RuntimeManifestEntry>,
    /// When the auto-update flow last ran a check. Used to honour the
    /// per-profile cadence ("weekly", "monthly"). RFC §4.8.
    #[serde(default)]
    pub last_auto_check: Option<DateTime<Utc>>,
}

impl RuntimeManifest {
    /// Empty manifest at the current schema_version. Used on first install.
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            schemas: Vec::new(),
            last_auto_check: None,
        }
    }

    /// Load from disk. Returns an empty manifest if the file does not exist —
    /// callers should treat that as "no schemas cached yet" (Article IV: this
    /// IS a loud signal upstream — `terrashift migrate` against an empty cache
    /// fails with a clean error pointing at `terrashift schema update`).
    pub fn load(path: &Path) -> Result<Self, SchemaError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::empty()),
            Err(e) => Err(SchemaError::ManifestIo(e.to_string())),
        }
    }

    /// Atomic write: serialize to `<path>.tmp` then rename. The rename is
    /// atomic on every supported OS; partial writes are never visible to a
    /// concurrent reader.
    pub fn save(&self, path: &Path) -> Result<(), SchemaError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SchemaError::ManifestIo(e.to_string()))?;
        }
        let tmp = path.with_extension("tmp");
        let json = serde_json::to_string_pretty(self).map_err(SchemaError::Serialize)?;
        std::fs::write(&tmp, json).map_err(|e| SchemaError::ManifestIo(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| SchemaError::ManifestIo(e.to_string()))?;
        Ok(())
    }

    /// Look up a manifest entry by (provider, version). Returns `None` on miss.
    pub fn find(&self, provider: &str, version: &str) -> Option<&RuntimeManifestEntry> {
        self.schemas
            .iter()
            .find(|e| e.provider == provider && e.version == version)
    }

    /// Insert or replace a manifest entry. Article IX permits replacement only
    /// when `terrashift schema update` re-captures the same `(provider, version)`
    /// after a prior verify-fail — the prior entry's content_hash record on the
    /// audit chain is the immutable history (see `AuditPayload::SchemaCapture`).
    pub fn upsert(&mut self, entry: RuntimeManifestEntry) {
        if let Some(existing) = self
            .schemas
            .iter_mut()
            .find(|e| e.provider == entry.provider && e.version == entry.version)
        {
            *existing = entry;
        } else {
            self.schemas.push(entry);
        }
    }

    /// Remove every manifest entry for (provider, version) — used by
    /// `terrashift schema gc`. Returns true if anything was removed.
    pub fn remove(&mut self, provider: &str, version: &str) -> bool {
        let before = self.schemas.len();
        self.schemas
            .retain(|e| !(e.provider == provider && e.version == version));
        self.schemas.len() != before
    }

    /// Snapshot of every cached `(provider, version)` pair, sorted by provider
    /// then version (descending lexicographic — same convention as
    /// `LocalSchemaStore::list_versions`).
    pub fn cached_pairs(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .schemas
            .iter()
            .map(|e| (e.provider.clone(), e.version.clone()))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        pairs
    }
}

/// One row in the runtime manifest. Every captured schema produces exactly
/// one of these. Re-capture overwrites in place per `RuntimeManifest::upsert`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeManifestEntry {
    /// Provider key as Terraform sees it: `"aws"`, `"azurerm"`, `"google"`.
    pub provider: String,
    /// Full source identifier: `"hashicorp/aws"`. Used by the registry HTTP
    /// client when expanding constraints during `terrashift schema update`.
    pub source: String,
    /// Resolved concrete version: `"5.30.4"`.
    pub version: String,
    /// What the user asked for: `"~> 5.30"` or `"= 5.30.4"`. Recorded so that
    /// later runs can answer "is this the version that originally satisfied
    /// the user's pin?"
    pub version_constraint: String,
    /// When the schema was extracted from the provider binary.
    pub captured_at: DateTime<Utc>,
    /// Operation that produced this entry: `"schema_update_command"`,
    /// `"bundled"`, `"auto_update"`. Pairs with `AuditPayload::SchemaCapture::captured_via`.
    pub captured_by: String,
    /// Version of `terraform` (or `tofu`) that emitted the schema.
    pub terraform_version: String,
    /// SHA-256 of the on-disk `schema.json` (lowercase hex). Recomputed by
    /// `terrashift schema verify`; mismatch means the file was tampered with
    /// or corrupted in transit.
    pub sha256: String,
    /// Size of the `schema.json` in bytes. Recorded so `schema list` can show
    /// it without `stat`-ing the file.
    pub size_bytes: u64,
    /// Path to `schema.json` relative to the cache root. Almost always
    /// `"<provider>/<version>/schema.json"`; carried explicitly so a future
    /// re-layout doesn't break consumers.
    pub schema_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_entry(provider: &str, version: &str) -> RuntimeManifestEntry {
        RuntimeManifestEntry {
            provider: provider.to_string(),
            source: format!("hashicorp/{provider}"),
            version: version.to_string(),
            version_constraint: format!("= {version}"),
            captured_at: Utc::now(),
            captured_by: "schema_update_command".to_string(),
            terraform_version: "1.7.5".to_string(),
            sha256: "a".repeat(64),
            size_bytes: 41_258_032,
            schema_path: format!("{provider}/{version}/schema.json"),
        }
    }

    #[test]
    fn empty_manifest_has_current_schema_version() {
        let m = RuntimeManifest::empty();
        assert_eq!(m.schema_version, SCHEMA_VERSION);
        assert!(m.schemas.is_empty());
        assert!(m.last_auto_check.is_none());
    }

    #[test]
    fn load_missing_file_returns_empty_not_error() {
        let dir = tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = dir.path().join("manifest.json");
        let m = RuntimeManifest::load(&path).unwrap_or_else(|e| panic!("load: {e}"));
        assert!(m.schemas.is_empty());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = dir.path().join("manifest.json");

        let mut m = RuntimeManifest::empty();
        m.upsert(sample_entry("aws", "5.30.0"));
        m.upsert(sample_entry("google", "5.40.2"));
        m.save(&path).unwrap_or_else(|e| panic!("save: {e}"));

        let loaded = RuntimeManifest::load(&path).unwrap_or_else(|e| panic!("load: {e}"));
        assert_eq!(loaded.schemas.len(), 2);
        assert!(loaded.find("aws", "5.30.0").is_some());
        assert!(loaded.find("google", "5.40.2").is_some());
    }

    #[test]
    fn upsert_replaces_existing_pair_in_place() {
        let mut m = RuntimeManifest::empty();
        m.upsert(sample_entry("aws", "5.30.0"));
        let mut updated = sample_entry("aws", "5.30.0");
        updated.terraform_version = "1.8.0".to_string();
        m.upsert(updated);

        assert_eq!(m.schemas.len(), 1, "upsert must not duplicate");
        let entry = m.find("aws", "5.30.0").expect("entry present");
        assert_eq!(entry.terraform_version, "1.8.0");
    }

    #[test]
    fn remove_returns_true_when_pair_existed() {
        let mut m = RuntimeManifest::empty();
        m.upsert(sample_entry("aws", "5.30.0"));
        assert!(m.remove("aws", "5.30.0"));
        assert!(!m.remove("aws", "5.30.0"));
        assert!(m.schemas.is_empty());
    }

    #[test]
    fn cached_pairs_sorts_provider_asc_version_desc() {
        let mut m = RuntimeManifest::empty();
        m.upsert(sample_entry("google", "5.40.2"));
        m.upsert(sample_entry("aws", "5.20.0"));
        m.upsert(sample_entry("aws", "5.30.0"));

        let pairs = m.cached_pairs();
        assert_eq!(
            pairs,
            vec![
                ("aws".to_string(), "5.30.0".to_string()),
                ("aws".to_string(), "5.20.0".to_string()),
                ("google".to_string(), "5.40.2".to_string()),
            ]
        );
    }
}
