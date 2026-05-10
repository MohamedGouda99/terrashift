// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use terrashift_cost_optimizer::{
    AnalyzeConfig, CostCache, CostLookup, CostOptimizer, CostOptimizerError, ResourceCostQuery,
    ResourceCostResult,
};
use terrashift_engine::mapper::{MappedResource, MappingPlan};
use uuid::Uuid;

/// `(provider, resource_type)` → canned cost or canned failure message.
type StubTable = BTreeMap<(String, String), Result<f64, &'static str>>;

/// Fixture stub: keyed lookup returning canned costs by `(provider, type)`.
struct StubLookup {
    table: Mutex<StubTable>,
    calls: Arc<Mutex<u32>>,
}

impl StubLookup {
    fn new(entries: &[((&str, &str), f64)]) -> Self {
        let table = entries
            .iter()
            .map(|((p, t), v)| ((p.to_string(), t.to_string()), Ok(*v)))
            .collect();
        Self {
            table: Mutex::new(table),
            calls: Arc::new(Mutex::new(0)),
        }
    }

    fn with_failure(self, provider: &str, ty: &str, msg: &'static str) -> Self {
        self.table
            .lock()
            .unwrap()
            .insert((provider.to_string(), ty.to_string()), Err(msg));
        self
    }
}

#[async_trait]
impl CostLookup for StubLookup {
    async fn lookup(
        &self,
        query: &ResourceCostQuery,
    ) -> Result<ResourceCostResult, CostOptimizerError> {
        *self.calls.lock().unwrap() += 1;
        let key = (query.provider.clone(), query.resource_type.clone());
        match self.table.lock().unwrap().get(&key) {
            Some(Ok(v)) => Ok(ResourceCostResult {
                usd_per_month: *v,
                source_link: None,
            }),
            Some(Err(msg)) => Err(CostOptimizerError::Api((*msg).to_string())),
            None => Err(CostOptimizerError::Api(format!("unknown stub key {key:?}"))),
        }
    }
}

fn plan_with(pairs: &[(&str, &str, &str, &str)]) -> MappingPlan {
    let resources = pairs
        .iter()
        .map(|(saddr, sname, taddr_type, tname)| MappedResource {
            source_addr: format!("{saddr}.{sname}"),
            target_addr: format!("{taddr_type}.{tname}"),
            target_type: taddr_type.to_string(),
            target_name: tname.to_string(),
            attributes: Default::default(),
            dependencies: vec![],
        })
        .collect();
    MappingPlan {
        run_id: Uuid::new_v4(),
        source_provider: "aws".to_string(),
        target_provider: "azurerm".to_string(),
        resources,
    }
}

#[tokio::test]
async fn happy_path_totals_match_per_resource_sums() {
    let stub = StubLookup::new(&[
        (("aws", "aws_instance"), 10.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 12.0),
        (("aws", "aws_s3_bucket"), 0.5),
        (("azurerm", "azurerm_storage_account"), 0.7),
    ]);
    let plan = plan_with(&[
        (
            "aws_instance",
            "web",
            "azurerm_linux_virtual_machine",
            "web",
        ),
        ("aws_s3_bucket", "data", "azurerm_storage_account", "data"),
    ]);
    let opt = CostOptimizer::new(stub);
    let r = opt.analyze(&plan, &AnalyzeConfig::default()).await.unwrap();
    assert_eq!(r.line_items.len(), 2);
    assert!((r.source_total_usd_per_month - 10.5).abs() < 0.001);
    assert!((r.target_total_usd_per_month - 12.7).abs() < 0.001);
    assert!((r.delta_usd_per_month() - 2.2).abs() < 0.001);
    assert_eq!(r.skipped.len(), 0);
}

#[tokio::test]
async fn lookup_failure_in_strict_mode_propagates() {
    let stub = StubLookup::new(&[(("aws", "aws_instance"), 10.0)]).with_failure(
        "azurerm",
        "azurerm_linux_virtual_machine",
        "boom",
    );
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);
    let cfg = AnalyzeConfig {
        strict: true,
        ..AnalyzeConfig::default()
    };
    let opt = CostOptimizer::new(stub);
    let err = opt.analyze(&plan, &cfg).await.unwrap_err();
    assert!(matches!(err, CostOptimizerError::Api(_)));
}

#[tokio::test]
async fn lookup_failure_in_lenient_mode_records_skip() {
    let stub = StubLookup::new(&[(("aws", "aws_instance"), 10.0)]).with_failure(
        "azurerm",
        "azurerm_linux_virtual_machine",
        "boom",
    );
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);
    let cfg = AnalyzeConfig::default();
    let opt = CostOptimizer::new(stub);
    let r = opt.analyze(&plan, &cfg).await.unwrap();
    assert_eq!(r.skipped.len(), 1);
    assert!((r.source_total_usd_per_month - 10.0).abs() < 0.001);
    assert_eq!(r.target_total_usd_per_month, 0.0);
}

#[tokio::test]
async fn cache_hit_avoids_lookup_call() {
    let stub = StubLookup::new(&[
        (("aws", "aws_instance"), 10.0),
        (("azurerm", "azurerm_linux_virtual_machine"), 12.0),
    ]);
    let calls_handle = stub.calls.clone();
    let dir = tempfile::tempdir().unwrap();
    let cache = CostCache::new(dir.path().to_path_buf(), 24);
    let opt = CostOptimizer::new(stub).with_cache(cache);
    let plan = plan_with(&[(
        "aws_instance",
        "web",
        "azurerm_linux_virtual_machine",
        "web",
    )]);
    let _ = opt.analyze(&plan, &AnalyzeConfig::default()).await.unwrap();
    let calls_after_first = *calls_handle.lock().unwrap();
    let _ = opt.analyze(&plan, &AnalyzeConfig::default()).await.unwrap();
    let calls_after_second = *calls_handle.lock().unwrap();
    assert_eq!(
        calls_after_first, 2,
        "first run: 1 source + 1 target = 2 stub calls"
    );
    assert_eq!(
        calls_after_second, calls_after_first,
        "cache hit must skip stub call entirely"
    );
}

#[tokio::test]
async fn empty_plan_yields_zero_totals_and_no_items() {
    let stub = StubLookup::new(&[]);
    let plan = plan_with(&[]);
    let opt = CostOptimizer::new(stub);
    let r = opt.analyze(&plan, &AnalyzeConfig::default()).await.unwrap();
    assert_eq!(r.line_items.len(), 0);
    assert_eq!(r.skipped.len(), 0);
    assert!((r.source_total_usd_per_month - 0.0).abs() < f64::EPSILON);
}
