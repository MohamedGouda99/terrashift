// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use terrashift_cost_optimizer::CostCache;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DummyValue {
    n: i32,
    s: String,
}

#[test]
fn miss_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let c = CostCache::new(dir.path().to_path_buf(), 24);
    let v: Option<DummyValue> = c.get("nonexistent").unwrap();
    assert!(v.is_none());
}

#[test]
fn set_then_get_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let c = CostCache::new(dir.path().to_path_buf(), 24);
    let v = DummyValue {
        n: 42,
        s: "ok".to_string(),
    };
    c.set("k1", &v).unwrap();
    let got: DummyValue = c.get("k1").unwrap().unwrap();
    assert_eq!(got, v);
}

#[test]
fn expired_entry_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let c = CostCache::new(dir.path().to_path_buf(), 24);
    let v = DummyValue {
        n: 7,
        s: "old".to_string(),
    };
    let past = Utc::now() - ChronoDuration::hours(48);
    c.set_with_timestamp("k_old", &v, past, 24).unwrap();
    let got: Option<DummyValue> = c.get("k_old").unwrap();
    assert!(got.is_none(), "entry beyond ttl_hours must read as None");
}

#[test]
fn fresh_entry_with_short_ttl_works() {
    let dir = tempfile::tempdir().unwrap();
    let c = CostCache::new(dir.path().to_path_buf(), 24);
    let v = DummyValue {
        n: 1,
        s: "fresh".to_string(),
    };
    c.set_with_timestamp("k_fresh", &v, Utc::now() - ChronoDuration::minutes(30), 1)
        .unwrap();
    let got: DummyValue = c.get("k_fresh").unwrap().unwrap();
    assert_eq!(got, v);
}

#[test]
fn keys_are_sha256_so_no_collisions_for_similar_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let c = CostCache::new(dir.path().to_path_buf(), 24);
    let v1 = DummyValue {
        n: 1,
        s: "a".to_string(),
    };
    let v2 = DummyValue {
        n: 2,
        s: "b".to_string(),
    };
    c.set("aws|instance|us-east-1", &v1).unwrap();
    c.set("aws|instance|us-west-2", &v2).unwrap();
    let g1: DummyValue = c.get("aws|instance|us-east-1").unwrap().unwrap();
    let g2: DummyValue = c.get("aws|instance|us-west-2").unwrap().unwrap();
    assert_eq!(g1, v1);
    assert_eq!(g2, v2);
}
