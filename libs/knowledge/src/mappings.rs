// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Curated cross-cloud `MappingExample` bundle, baked into the binary.
//!
//! Source of truth: `seed/mappings.toml`. Article XIII rule 8 — data,
//! not source-code enumeration. Consumed by the Mapper as a strong
//! prior under "PROVEN EQUIVALENCES" in its prompt.

use crate::types::MappingExample;
use serde::Deserialize;
use std::sync::LazyLock;

const SEED: &str = include_str!("../seed/mappings.toml");

#[derive(Deserialize)]
struct Bundle {
    mappings: Vec<RawMapping>,
}

#[derive(Deserialize)]
struct RawMapping {
    source_provider: String,
    source_resource: String,
    target_provider: String,
    target_resource: String,
    #[serde(default)]
    attribute_alignments: Vec<Vec<String>>,
    confidence: f64,
}

impl From<RawMapping> for MappingExample {
    fn from(r: RawMapping) -> Self {
        let attribute_alignments = r
            .attribute_alignments
            .into_iter()
            .filter_map(|pair| {
                let mut it = pair.into_iter();
                Some((it.next()?, it.next()?))
            })
            .collect();
        MappingExample {
            source_provider: r.source_provider,
            source_resource: r.source_resource,
            target_provider: r.target_provider,
            target_resource: r.target_resource,
            attribute_alignments,
            confidence: r.confidence,
        }
    }
}

pub static CURATED_MAPPINGS: LazyLock<Vec<MappingExample>> = LazyLock::new(|| {
    // Compile-bundled data; mappings::tests::bundle_parses guards against
    // a malformed file landing in main. Runtime parse failure here is
    // effectively unreachable, but Article XIII rule 3 forbids panic
    // shortcuts — log + return empty on the impossible case.
    match toml::from_str::<Bundle>(SEED) {
        Ok(bundle) => bundle
            .mappings
            .into_iter()
            .map(MappingExample::from)
            .collect(),
        Err(e) => {
            tracing::error!("curated mappings bundle failed to parse (build-time data, this is a code bug): {e}");
            Vec::new()
        }
    }
});

/// Curated mapping examples for the given direction. Returns an empty
/// slice when the direction is unknown.
pub fn find_curated(source_provider: &str, target_provider: &str) -> Vec<&'static MappingExample> {
    CURATED_MAPPINGS
        .iter()
        .filter(|m| m.source_provider == source_provider && m.target_provider == target_provider)
        .collect()
}

/// Look up the canonical target resource for a specific source resource
/// in a given direction. Returns the highest-confidence match.
pub fn canonical_target(
    source_provider: &str,
    source_resource: &str,
    target_provider: &str,
) -> Option<&'static MappingExample> {
    CURATED_MAPPINGS
        .iter()
        .filter(|m| {
            m.source_provider == source_provider
                && m.source_resource == source_resource
                && m.target_provider == target_provider
        })
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_parses() {
        assert!(
            !CURATED_MAPPINGS.is_empty(),
            "curated mappings should load from seed/mappings.toml"
        );
    }

    #[test]
    fn six_directions_covered() {
        let directions = [
            ("aws", "azurerm"),
            ("azurerm", "aws"),
            ("aws", "google"),
            ("google", "aws"),
            ("azurerm", "google"),
            ("google", "azurerm"),
        ];
        for (src, dst) in directions {
            let pairs = find_curated(src, dst);
            assert!(
                !pairs.is_empty(),
                "{src} → {dst}: expected ≥1 curated pair, found 0"
            );
        }
    }

    #[test]
    fn canonical_aws_vpc_to_azurerm_is_virtual_network() {
        let m = canonical_target("aws", "aws_vpc", "azurerm").expect("aws_vpc → azurerm exists");
        assert_eq!(m.target_resource, "azurerm_virtual_network");
        assert!(m.confidence >= 0.9);
    }
}
