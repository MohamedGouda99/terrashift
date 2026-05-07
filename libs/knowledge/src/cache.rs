// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Hot-path schema cache — memoizes parsed `ProviderSchema` per (provider, version).
//!
//! The runtime manifest at `~/.terrashift/schemas/manifest.json` is read at
//! startup. The actual `schema.json` files (~40 MB for AWS) are loaded lazily
//! on first reference and held as `Arc<ProviderSchema>` for the life of the
//! migration. `Arc::clone` shares the parsed tree across worker threads at
//! zero copy cost.
//!
//! Pattern: terrashift_plan.md §7 (knowledge layer hot-path). RFC §4.5
//! mandates `serde_json::from_slice` + `Arc<ProviderSchema>` (NOT `memmap2` —
//! a typed deserialise walks every byte regardless of how the input was
//! obtained).
//!
//! Constitution: Article XII rule 2 (cache-first; subsequent reads of the
//! same (provider, version) hit the dashmap, not the disk).

use crate::errors::SchemaError;
use crate::types::ProviderSchema;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, instrument};

/// Hot-path cache. Owns the cache root path; the manifest itself is owned
/// by `KnowledgeService` so multiple cache instances aren't possible by
/// construction (no concurrent-modifier hazards on the manifest file).
pub struct RuntimeSchemaCache {
    cache_root: PathBuf,
    parsed: DashMap<(String, String), Arc<ProviderSchema>>,
}

impl RuntimeSchemaCache {
    /// Construct an empty cache rooted at the given directory. The directory
    /// is NOT created here — first-run extraction of bundled schemas is the
    /// caller's job (see `extract_bundled_schemas` in the CLI).
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
            parsed: DashMap::new(),
        }
    }

    /// Return the on-disk root the cache reads from.
    pub fn root(&self) -> &Path {
        &self.cache_root
    }

    /// How many parsed schemas are currently in memory. Useful for the TUI
    /// status footer and for tests.
    pub fn len_parsed(&self) -> usize {
        self.parsed.len()
    }

    /// Hot-path read. First call per (provider, version) reads disk + parses;
    /// every subsequent call returns a clone of the cached `Arc<ProviderSchema>`.
    ///
    /// The ~40 MB JSON parse is a hard ceiling per RFC §4.5: under no
    /// circumstances should the migrate hot path do this on every invocation.
    /// If a benchmark shows it does, the consumer is bypassing this cache.
    #[instrument(skip(self), fields(provider, version))]
    pub async fn load(
        &self,
        provider: &str,
        version: &str,
    ) -> Result<Arc<ProviderSchema>, SchemaError> {
        let key = (provider.to_string(), version.to_string());

        if let Some(hit) = self.parsed.get(&key) {
            debug!("cache hit");
            return Ok(Arc::clone(hit.value()));
        }

        // Cold path: read + parse + insert. dashmap's get-or-insert race is
        // resolved by the second writer's `insert` overwriting the first;
        // both threads end up with structurally-equivalent Arcs (same JSON
        // bytes, same parse). The wasted work is bounded to one parse.
        let path = self
            .cache_root
            .join(provider)
            .join(version)
            .join("schema.json");

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| SchemaError::ManifestIo(format!("read {}: {e}", path.display())))?;

        let schema: ProviderSchema = serde_json::from_slice(&bytes)?;
        let arc = Arc::new(schema);

        self.parsed.insert(key, Arc::clone(&arc));
        debug!("cache populated from disk");
        Ok(arc)
    }

    /// Drop the cached parse for a (provider, version) pair. Used by
    /// `terrashift schema gc` after the on-disk files are removed, so that
    /// subsequent reads loudly fail rather than serving stale memory.
    pub fn invalidate(&self, provider: &str, version: &str) {
        self.parsed
            .remove(&(provider.to_string(), version.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AttributeSchema, ResourceSchema};
    use chrono::Utc;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn sample_schema(provider: &str, version: &str) -> ProviderSchema {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "cidr_block".to_string(),
            AttributeSchema {
                name: "cidr_block".to_string(),
                attribute_type: "string".to_string(),
                required: true,
                optional: false,
                computed: false,
                sensitive: false,
                deprecated: None,
                description: Some("desc".to_string()),
            },
        );
        let mut resources = BTreeMap::new();
        resources.insert(
            format!("{provider}_vpc"),
            ResourceSchema {
                name: format!("{provider}_vpc"),
                description: Some("vpc".to_string()),
                attributes: attrs,
            },
        );
        ProviderSchema {
            provider: provider.to_string(),
            version: version.to_string(),
            resources,
            data_sources: BTreeMap::new(),
            fetched_at: Utc::now(),
        }
    }

    fn write_schema_file(root: &Path, provider: &str, version: &str) {
        let dir = root.join(provider).join(version);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let json = serde_json::to_string(&sample_schema(provider, version)).expect("ser");
        std::fs::write(dir.join("schema.json"), json).expect("write");
    }

    #[tokio::test]
    async fn load_reads_disk_on_first_call() {
        let dir = tempdir().expect("tempdir");
        write_schema_file(dir.path(), "aws", "5.30.0");

        let cache = RuntimeSchemaCache::new(dir.path());
        assert_eq!(cache.len_parsed(), 0);

        let schema = cache.load("aws", "5.30.0").await.expect("load");
        assert_eq!(schema.provider, "aws");
        assert_eq!(cache.len_parsed(), 1);
    }

    #[tokio::test]
    async fn load_serves_cached_arc_on_second_call() {
        let dir = tempdir().expect("tempdir");
        write_schema_file(dir.path(), "aws", "5.30.0");

        let cache = RuntimeSchemaCache::new(dir.path());
        let a = cache.load("aws", "5.30.0").await.expect("load 1");
        let b = cache.load("aws", "5.30.0").await.expect("load 2");

        // Same Arc — same allocation, same parse. Article XII rule 2.
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(cache.len_parsed(), 1);
    }

    #[tokio::test]
    async fn load_missing_file_is_typed_error() {
        let dir = tempdir().expect("tempdir");
        let cache = RuntimeSchemaCache::new(dir.path());
        let err = cache.load("aws", "999.0.0").await.expect_err("must fail");
        match err {
            SchemaError::ManifestIo(msg) => assert!(msg.contains("999.0.0")),
            other => panic!("expected ManifestIo, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalidate_drops_cached_arc() {
        let dir = tempdir().expect("tempdir");
        write_schema_file(dir.path(), "aws", "5.30.0");

        let cache = RuntimeSchemaCache::new(dir.path());
        cache.load("aws", "5.30.0").await.expect("load");
        assert_eq!(cache.len_parsed(), 1);

        cache.invalidate("aws", "5.30.0");
        assert_eq!(cache.len_parsed(), 0);
    }
}
