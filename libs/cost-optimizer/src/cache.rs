// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! File-backed cost cache.
//!
//! Layout: `<root>/<sha256(key)>.json` with envelope:
//! ```json
//! { "stored_at": "2026-05-10T12:00:00Z", "ttl_hours": 24, "value": <T> }
//! ```
//!
//! TOCTOU-safe reads (mirror libs/feedback/src/optin.rs pattern):
//! we attempt the read directly and match on `ErrorKind::NotFound`
//! rather than `path.exists()` + `read_to_string`.
//!
//! Article V: cache keys are sha256 of stable inputs. Resource attribute
//! values that could carry secrets (tag values, bucket names, IPs) MUST
//! NOT be in the input string used to derive the key. The caller is
//! responsible for passing a privacy-safe key fragment; this layer
//! just hashes whatever it's given.

use chrono::{DateTime, Duration, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CostCacheError {
    #[error("io {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(PathBuf, #[source] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope<T> {
    stored_at: DateTime<Utc>,
    ttl_hours: i64,
    value: T,
}

pub struct CostCache {
    root: PathBuf,
    default_ttl_hours: i64,
}

impl CostCache {
    pub fn new(root: PathBuf, default_ttl_hours: i64) -> Self {
        Self {
            root,
            default_ttl_hours,
        }
    }

    fn key_path(&self, key: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        self.root.join(format!("{hex}.json"))
    }

    /// Return the cached value if present and not expired; `Ok(None)`
    /// otherwise. Loud on parse / io errors that aren't NotFound.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CostCacheError> {
        let path = self.key_path(key);
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CostCacheError::Io(path, e)),
        };
        let env: Envelope<T> =
            serde_json::from_str(&raw).map_err(|e| CostCacheError::Parse(path.clone(), e))?;
        let age = Utc::now() - env.stored_at;
        if age > Duration::hours(env.ttl_hours) {
            return Ok(None);
        }
        Ok(Some(env.value))
    }

    /// Write an entry with the cache's default TTL.
    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), CostCacheError> {
        let path = self.key_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CostCacheError::Io(parent.to_path_buf(), e))?;
        }
        let env = Envelope {
            stored_at: Utc::now(),
            ttl_hours: self.default_ttl_hours,
            value,
        };
        let raw = serde_json::to_string_pretty(&env)
            .map_err(|e| CostCacheError::Parse(path.clone(), e))?;
        std::fs::write(&path, raw).map_err(|e| CostCacheError::Io(path, e))?;
        Ok(())
    }

    /// Used by tests to inject an artificially-old entry.
    #[doc(hidden)]
    pub fn set_with_timestamp<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        stored_at: DateTime<Utc>,
        ttl_hours: i64,
    ) -> Result<(), CostCacheError> {
        let path = self.key_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CostCacheError::Io(parent.to_path_buf(), e))?;
        }
        let env = Envelope {
            stored_at,
            ttl_hours,
            value,
        };
        let raw = serde_json::to_string_pretty(&env)
            .map_err(|e| CostCacheError::Parse(path.clone(), e))?;
        std::fs::write(&path, raw).map_err(|e| CostCacheError::Io(path, e))?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}
