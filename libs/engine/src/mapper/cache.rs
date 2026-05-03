//! `MapperCache` + `estate_cache_key` — Article XII rule 2 enforcement.
//!
//! Pattern: the reference codebase (see ATTRIBUTIONS.md)
//! (`compute_plan_hash` — `Sha256::new() → update(bytes) →
//! format!("{:x}", finalize())`). Identical idiom; the cache key
//! is the canonical-JSON hash of the input.
//!
//! Stage 1: in-process `HashMap<String, MappingPlan>`. No eviction.
//! Stage 2+ may add disk-backed cache + LRU eviction; the
//! `MapperCache` API stays the same.
//!
//! Constitution: Article VI (cache stability — same inventory always
//! produces the same cache key), Article XII rule 2 (cache-first:
//! lookup before LLM call).

use crate::mapper::{MapperError, MappingPlan};
use crate::scanner::EstateInventory;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Compute the canonical cache key for an `EstateInventory`. Sha256 of
/// `serde_json::to_vec(estate)`, hex-encoded. Stable across runs
/// because:
/// 1. `EstateInventory` derives `Serialize` deterministically (Scanner
///    emits resources in walk order).
/// 2. `serde_json::to_vec` produces canonical bytes (no whitespace
///    drift between runs of the same compiler version).
/// 3. Sha256 is byte-stable.
pub fn estate_cache_key(estate: &EstateInventory) -> Result<String, MapperError> {
    let bytes = serde_json::to_vec(estate).map_err(|e| MapperError::Serialize(Box::new(e)))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// In-process `MappingPlan` cache. Stage 1 minimal — no eviction.
#[derive(Debug, Default)]
pub struct MapperCache {
    entries: HashMap<String, MappingPlan>,
}

impl MapperCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Cache lookup. `None` on miss.
    pub fn get(&self, key: &str) -> Option<&MappingPlan> {
        self.entries.get(key)
    }

    /// Cache insert. Returns the previous entry (if any) for
    /// observability — Stage 1 callers don't use it.
    pub fn insert(&mut self, key: String, plan: MappingPlan) -> Option<MappingPlan> {
        self.entries.insert(key, plan)
    }

    /// Number of cached plans. Used by tests + future diagnostics.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
